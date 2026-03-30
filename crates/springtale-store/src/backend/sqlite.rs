use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use crate::error::StoreError;
use crate::migrations;
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::jobs::{JobId, JobRow};
use springtale_core::rule::types::{Rule, RuleId};

/// SQLite-backed storage. Single-file, zero external dependencies.
///
/// Connection is wrapped in `tokio::sync::Mutex` because `rusqlite` is
/// synchronous. All trait methods acquire the lock, do sync work, then
/// release. This is acceptable for single-user local deployments.
pub struct SqliteBackend {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl SqliteBackend {
    /// Open or create a SQLite database at the given path.
    ///
    /// Sets file permissions to 0o600, enables WAL mode, and runs migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        #[cfg(unix)]
        set_db_permissions(path)?;

        configure_connection(&conn)?;
        migrations::run_migrations(&conn)?;

        tracing::info!(path = %path.display(), "SQLite store opened");

        Ok(Self {
            conn: Mutex::new(conn),
            path: Some(path.to_owned()),
        })
    }

    /// Open an in-memory SQLite database (for testing).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        migrations::run_migrations(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            path: None,
        })
    }

    /// Get the database file path (None for in-memory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// Configure SQLite connection pragmas.
fn configure_connection(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

/// Set file permissions to 0o600 (owner read/write only).
#[cfg(unix)]
fn set_db_permissions(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[async_trait]
impl super::trait_::StorageBackend for SqliteBackend {
    // ── Rules ──────────────────────────────────────────────────

    async fn insert_rule(&self, rule: &Rule) -> Result<RuleId, StoreError> {
        let conn = self.conn.lock().await;
        let rule_toml =
            toml::to_string(rule).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO rules (id, name, description, status, version, trigger_type, rule_toml, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                rule.id.0.to_string(),
                rule.name,
                rule.description,
                format!("{:?}", rule.status).to_lowercase(),
                rule.version.0 as i64,
                rule.trigger.trigger_type(),
                rule_toml,
                now,
                now,
            ],
        )?;

        Ok(rule.id)
    }

    async fn find_rules_by_trigger(&self, trigger_type: &str) -> Result<Vec<Rule>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT rule_toml FROM rules WHERE trigger_type = ?1 AND status = 'enabled'",
        )?;
        let rows = stmt
            .query_map(params![trigger_type], |row| {
                let toml_str: String = row.get(0)?;
                Ok(toml_str)
            })?
            .collect::<Result<Vec<String>, _>>()?;

        let mut rules = Vec::new();
        for toml_str in rows {
            let rule: Rule =
                toml::from_str(&toml_str).map_err(|e| StoreError::Serialization(e.to_string()))?;
            rules.push(rule);
        }
        Ok(rules)
    }

    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT rule_toml FROM rules ORDER BY created_at")?;
        let rows = stmt
            .query_map([], |row| {
                let toml_str: String = row.get(0)?;
                Ok(toml_str)
            })?
            .collect::<Result<Vec<String>, _>>()?;

        let mut rules = Vec::new();
        for toml_str in rows {
            let rule: Rule =
                toml::from_str(&toml_str).map_err(|e| StoreError::Serialization(e.to_string()))?;
            rules.push(rule);
        }
        Ok(rules)
    }

    async fn toggle_rule(&self, id: &RuleId, enabled: bool) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let status = if enabled { "enabled" } else { "disabled" };
        let updated = conn.execute(
            "UPDATE rules SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, Utc::now().to_rfc3339(), id.0.to_string()],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound {
                entity: "rule".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete_rule(&self, id: &RuleId) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute("DELETE FROM rules WHERE id = ?1", params![id.0.to_string()])?;
        if deleted == 0 {
            return Err(StoreError::NotFound {
                entity: "rule".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }

    // ── Connectors ─────────────────────────────────────────────

    async fn register_connector(&self, row: &ConnectorRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO connectors (name, version, author, description, manifest_json, enabled, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.name,
                row.version,
                row.author,
                row.description,
                row.manifest_json,
                row.enabled,
                row.installed_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn list_connectors(&self) -> Result<Vec<ConnectorRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT name, version, author, description, manifest_json, enabled, installed_at FROM connectors ORDER BY name")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ConnectorRow {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    author: row.get(2)?,
                    description: row.get(3)?,
                    manifest_json: row.get(4)?,
                    enabled: row.get(5)?,
                    installed_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                6,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                })
            })?
            .collect::<Result<Vec<ConnectorRow>, _>>()?;
        Ok(rows)
    }

    async fn set_connector_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE connectors SET enabled = ?1 WHERE name = ?2",
            params![enabled, name],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound {
                entity: "connector".into(),
                id: name.to_owned(),
            });
        }
        Ok(())
    }

    async fn remove_connector(&self, name: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute("DELETE FROM connectors WHERE name = ?1", params![name])?;
        if deleted == 0 {
            return Err(StoreError::NotFound {
                entity: "connector".into(),
                id: name.to_owned(),
            });
        }
        Ok(())
    }

    // ── Events ─────────────────────────────────────────────────

    async fn log_event(&self, event: &EventEntry) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO events (id, connector_name, trigger_type, timestamp, action_taken)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.id.to_string(),
                event.connector_name,
                event.trigger_type,
                event.timestamp.to_rfc3339(),
                event.action_taken,
            ],
        )?;
        Ok(())
    }

    async fn list_events(&self, filter: &EventFilter) -> Result<Vec<EventEntry>, StoreError> {
        let conn = self.conn.lock().await;

        let mut sql = String::from(
            "SELECT id, connector_name, trigger_type, timestamp, action_taken FROM events WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref name) = filter.connector_name {
            sql.push_str(&format!(" AND connector_name = ?{param_idx}"));
            param_values.push(Box::new(name.clone()));
            param_idx += 1;
        }
        if let Some(ref tt) = filter.trigger_type {
            sql.push_str(&format!(" AND trigger_type = ?{param_idx}"));
            param_values.push(Box::new(tt.clone()));
            param_idx += 1;
        }
        if let Some(ref after) = filter.after {
            sql.push_str(&format!(" AND timestamp > ?{param_idx}"));
            param_values.push(Box::new(after.to_rfc3339()));
            param_idx += 1;
        }
        if let Some(ref before) = filter.before {
            sql.push_str(&format!(" AND timestamp < ?{param_idx}"));
            param_values.push(Box::new(before.to_rfc3339()));
            param_idx += 1;
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT ?{param_idx}"));
            param_values.push(Box::new(limit as i64));
            param_idx += 1;
        }

        if let Some(offset) = filter.offset {
            sql.push_str(&format!(" OFFSET ?{param_idx}"));
            param_values.push(Box::new(offset as i64));
            let _ = param_idx; // suppress unused warning
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id_str: String = row.get(0)?;
                let ts_str: String = row.get(3)?;
                Ok((id_str, row.get(1)?, row.get(2)?, ts_str, row.get(4)?))
            })?
            .collect::<Result<Vec<(String, String, String, String, String)>, _>>()?;

        let mut events = Vec::new();
        for (id_str, connector_name, trigger_type, ts_str, action_taken) in rows {
            let id = uuid::Uuid::parse_str(&id_str)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            events.push(EventEntry {
                id,
                connector_name,
                trigger_type,
                timestamp,
                action_taken,
            });
        }
        Ok(events)
    }

    async fn delete_events_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            params![before.to_rfc3339()],
        )?;
        Ok(deleted as u64)
    }

    // ── Jobs ───────────────────────────────────────────────────

    async fn enqueue_job(&self, job: &JobRow) -> Result<JobId, StoreError> {
        let conn = self.conn.lock().await;
        let payload_str = serde_json::to_string(&job.payload)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        conn.execute(
            "INSERT INTO jobs (id, payload, status, attempts, max_attempts, created_at, started_at, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                job.id.0.to_string(),
                payload_str,
                job.status,
                job.attempts as i64,
                job.max_attempts as i64,
                job.created_at.to_rfc3339(),
                job.started_at.as_ref().map(|t| t.to_rfc3339()),
                job.last_error,
            ],
        )?;
        Ok(job.id)
    }

    async fn dequeue_job(&self) -> Result<Option<JobRow>, StoreError> {
        let conn = self.conn.lock().await;

        // Find the oldest pending job and mark it as running in one step
        let now = Utc::now().to_rfc3339();

        // First find the job
        let maybe_id: Option<String> = conn
            .query_row(
                "SELECT id FROM jobs WHERE status = 'pending' ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let Some(job_id) = maybe_id else {
            return Ok(None);
        };

        // Mark it as running
        conn.execute(
            "UPDATE jobs SET status = 'running', started_at = ?1, attempts = attempts + 1 WHERE id = ?2",
            params![now, job_id],
        )?;

        // Read the full job back
        let row = conn.query_row(
            "SELECT id, payload, status, attempts, max_attempts, created_at, started_at, last_error FROM jobs WHERE id = ?1",
            params![job_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )?;

        let id =
            uuid::Uuid::parse_str(&row.0).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let payload: serde_json::Value =
            serde_json::from_str(&row.1).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&row.5)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let started_at = row
            .6
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(Some(JobRow {
            id: JobId(id),
            payload,
            status: row.2,
            attempts: row.3 as u32,
            max_attempts: row.4 as u32,
            created_at,
            started_at,
            last_error: row.7,
        }))
    }

    async fn complete_job(&self, id: &JobId) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE jobs SET status = 'complete' WHERE id = ?1",
            params![id.0.to_string()],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound {
                entity: "job".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn fail_job(&self, id: &JobId, error: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let updated = conn.execute(
            "UPDATE jobs SET status = 'failed', last_error = ?1 WHERE id = ?2",
            params![error, id.0.to_string()],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound {
                entity: "job".into(),
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::trait_::StorageBackend;
    use springtale_core::rule::action::Action;
    use springtale_core::rule::trigger::Trigger;
    use springtale_core::rule::types::{RuleStatus, RuleVersion};

    fn test_rule(name: &str) -> Rule {
        Rule {
            id: RuleId::new(),
            name: name.into(),
            description: "test rule".into(),
            status: RuleStatus::Enabled,
            version: RuleVersion(1),
            trigger: Trigger::Cron {
                expression: "0 9 * * *".into(),
            },
            conditions: vec![],
            actions: vec![Action::SendMessage {
                text: "hello".into(),
            }],
        }
    }

    #[tokio::test]
    async fn test_insert_and_list_rules() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let rule = test_rule("test-rule");
        let id = store.insert_rule(&rule).await.unwrap();

        let rules = store.list_rules().await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "test-rule");
        assert_eq!(rules[0].id, id);
    }

    #[tokio::test]
    async fn test_find_rules_by_trigger() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store.insert_rule(&test_rule("cron-rule")).await.unwrap();

        let found = store.find_rules_by_trigger("Cron").await.unwrap();
        assert_eq!(found.len(), 1);

        let not_found = store.find_rules_by_trigger("FileWatch").await.unwrap();
        assert!(not_found.is_empty());
    }

    #[tokio::test]
    async fn test_toggle_rule() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let rule = test_rule("toggle-rule");
        let id = store.insert_rule(&rule).await.unwrap();

        store.toggle_rule(&id, false).await.unwrap();
        let rules = store.find_rules_by_trigger("Cron").await.unwrap();
        assert!(rules.is_empty()); // disabled rules not returned by find_by_trigger
    }

    #[tokio::test]
    async fn test_delete_rule() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let rule = test_rule("delete-rule");
        let id = store.insert_rule(&rule).await.unwrap();

        store.delete_rule(&id).await.unwrap();
        let rules = store.list_rules().await.unwrap();
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn test_register_and_list_connectors() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let row = ConnectorRow {
            name: "connector-test".into(),
            version: "1.0.0".into(),
            author: "test".into(),
            description: "a test connector".into(),
            manifest_json: r#"{"name":"connector-test"}"#.into(),
            enabled: true,
            installed_at: Utc::now(),
        };
        store.register_connector(&row).await.unwrap();

        let connectors = store.list_connectors().await.unwrap();
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].name, "connector-test");
        assert!(connectors[0].enabled);
    }

    #[tokio::test]
    async fn test_set_connector_enabled() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let row = ConnectorRow {
            name: "connector-toggle".into(),
            version: "1.0.0".into(),
            author: "test".into(),
            description: String::new(),
            manifest_json: "{}".into(),
            enabled: true,
            installed_at: Utc::now(),
        };
        store.register_connector(&row).await.unwrap();
        store
            .set_connector_enabled("connector-toggle", false)
            .await
            .unwrap();

        let connectors = store.list_connectors().await.unwrap();
        assert!(!connectors[0].enabled);
    }

    #[tokio::test]
    async fn test_log_and_list_events() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let event = EventEntry {
            id: uuid::Uuid::new_v4(),
            connector_name: "connector-kick".into(),
            trigger_type: "ConnectorEvent".into(),
            timestamp: Utc::now(),
            action_taken: "sent message".into(),
        };
        store.log_event(&event).await.unwrap();

        let events = store
            .list_events(&EventFilter {
                connector_name: Some("connector-kick".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action_taken, "sent message");
    }

    #[tokio::test]
    async fn test_delete_events_before() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let old_event = EventEntry {
            id: uuid::Uuid::new_v4(),
            connector_name: "test".into(),
            trigger_type: "Cron".into(),
            timestamp: Utc::now() - chrono::Duration::days(30),
            action_taken: "old".into(),
        };
        let new_event = EventEntry {
            id: uuid::Uuid::new_v4(),
            connector_name: "test".into(),
            trigger_type: "Cron".into(),
            timestamp: Utc::now(),
            action_taken: "new".into(),
        };
        store.log_event(&old_event).await.unwrap();
        store.log_event(&new_event).await.unwrap();

        let deleted = store
            .delete_events_before(&(Utc::now() - chrono::Duration::days(7)))
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let remaining = store.list_events(&EventFilter::default()).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].action_taken, "new");
    }

    #[tokio::test]
    async fn test_enqueue_and_dequeue_job() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let job = JobRow {
            id: JobId::new(),
            payload: serde_json::json!({"action": "test"}),
            status: "pending".into(),
            attempts: 0,
            max_attempts: 3,
            created_at: Utc::now(),
            started_at: None,
            last_error: None,
        };
        store.enqueue_job(&job).await.unwrap();

        let dequeued = store.dequeue_job().await.unwrap();
        assert!(dequeued.is_some());
        let dequeued = dequeued.unwrap();
        assert_eq!(dequeued.status, "running");
        assert_eq!(dequeued.attempts, 1);
    }

    #[tokio::test]
    async fn test_complete_job() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let job = JobRow {
            id: JobId::new(),
            payload: serde_json::json!({}),
            status: "pending".into(),
            attempts: 0,
            max_attempts: 1,
            created_at: Utc::now(),
            started_at: None,
            last_error: None,
        };
        let id = store.enqueue_job(&job).await.unwrap();
        store.dequeue_job().await.unwrap();

        store.complete_job(&id).await.unwrap();

        // No more pending jobs
        let next = store.dequeue_job().await.unwrap();
        assert!(next.is_none());
    }

    #[tokio::test]
    async fn test_fail_job() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let job = JobRow {
            id: JobId::new(),
            payload: serde_json::json!({}),
            status: "pending".into(),
            attempts: 0,
            max_attempts: 1,
            created_at: Utc::now(),
            started_at: None,
            last_error: None,
        };
        let id = store.enqueue_job(&job).await.unwrap();
        store.dequeue_job().await.unwrap();

        store.fail_job(&id, "something broke").await.unwrap();

        let next = store.dequeue_job().await.unwrap();
        assert!(next.is_none());
    }

    #[tokio::test]
    async fn test_dequeue_empty_returns_none() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let result = store.dequeue_job().await.unwrap();
        assert!(result.is_none());
    }
}
