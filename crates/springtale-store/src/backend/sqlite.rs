use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use crate::error::StoreError;
use crate::migrations;
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::formations::{FormationMemberRow, FormationRow};
use crate::schema::safety::SafetyConfigRow;
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

    // ── Bot Sessions ──────────────────────────────────────────

    async fn upsert_session(&self, session: &SessionRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO bot_sessions (user_id, channel_id, last_bot_message, pending_command, state_data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(user_id, channel_id) DO UPDATE SET
                last_bot_message = excluded.last_bot_message,
                pending_command = excluded.pending_command,
                state_data = excluded.state_data,
                updated_at = excluded.updated_at",
            params![
                session.user_id,
                session.channel_id,
                session.last_bot_message,
                session.pending_command,
                session.state_data,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_session(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT user_id, channel_id, last_bot_message, pending_command, state_data, created_at, updated_at
             FROM bot_sessions WHERE user_id = ?1 AND channel_id = ?2",
            params![user_id, channel_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        );

        match result {
            Ok((uid, cid, last_msg, pending, state, created, updated)) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(SessionRow {
                    user_id: uid,
                    channel_id: cid,
                    last_bot_message: last_msg,
                    pending_command: pending,
                    state_data: state,
                    created_at,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete_session(&self, user_id: &str, channel_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM bot_sessions WHERE user_id = ?1 AND channel_id = ?2",
            params![user_id, channel_id],
        )?;
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT user_id, channel_id, last_bot_message, pending_command, \
             state_data, created_at, updated_at \
             FROM bot_sessions ORDER BY updated_at DESC",
        )?;
        let tuples: Vec<(
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
        )> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut sessions = Vec::with_capacity(tuples.len());
        for (uid, cid, last_msg, pending, state, created, updated) in tuples {
            let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            sessions.push(SessionRow {
                user_id: uid,
                channel_id: cid,
                last_bot_message: last_msg,
                pending_command: pending,
                state_data: state,
                created_at,
                updated_at,
            });
        }
        Ok(sessions)
    }

    // ── User Preferences ──────────────────────────────────────

    async fn upsert_user_prefs(&self, prefs: &UserPrefsRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO user_prefs (user_id, timezone, language, notifications_enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET
                timezone = excluded.timezone,
                language = excluded.language,
                notifications_enabled = excluded.notifications_enabled,
                updated_at = excluded.updated_at",
            params![
                prefs.user_id,
                prefs.timezone,
                prefs.language,
                prefs.notifications_enabled,
                prefs.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_user_prefs(&self, user_id: &str) -> Result<Option<UserPrefsRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT user_id, timezone, language, notifications_enabled, updated_at
             FROM user_prefs WHERE user_id = ?1",
            params![user_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        );

        match result {
            Ok((uid, tz, lang, notif, updated)) => {
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(UserPrefsRow {
                    user_id: uid,
                    timezone: tz,
                    language: lang,
                    notifications_enabled: notif,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ── Bot Memory ────────────────────────────────────────────

    async fn insert_memory(&self, entry: &MemoryRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO bot_memory (id, user_id, channel_id, category, schema_version, author, source,
             content_encrypted, nonce, content_hash, parent_id, trust_score, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                entry.id,
                entry.user_id,
                entry.channel_id,
                entry.category,
                entry.schema_version,
                entry.author,
                entry.source,
                entry.content_encrypted,
                entry.nonce,
                entry.content_hash,
                entry.parent_id,
                entry.trust_score,
                entry.created_at.to_rfc3339(),
                entry.expires_at.as_ref().map(|t| t.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    async fn get_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, user_id, channel_id, category, schema_version, author, source,
                    content_encrypted, nonce, content_hash, parent_id, trust_score,
                    created_at, expires_at
             FROM bot_memory
             WHERE user_id = ?1 AND channel_id = ?2
             ORDER BY created_at DESC
             LIMIT ?3",
        )?;

        let rows = stmt
            .query_map(params![user_id, channel_id, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, f64>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut entries = Vec::with_capacity(rows.len());
        for r in rows {
            let created_at = chrono::DateTime::parse_from_rfc3339(&r.12)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let expires_at =
                r.13.as_ref()
                    .map(|s| {
                        chrono::DateTime::parse_from_rfc3339(s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .map_err(|e| StoreError::Serialization(e.to_string()))
                    })
                    .transpose()?;

            entries.push(MemoryRow {
                id: r.0,
                user_id: r.1,
                channel_id: r.2,
                category: r.3,
                schema_version: r.4,
                author: r.5,
                source: r.6,
                content_encrypted: r.7,
                nonce: r.8,
                content_hash: r.9,
                parent_id: r.10,
                trust_score: r.11,
                created_at,
                expires_at,
            });
        }
        Ok(entries)
    }

    async fn delete_memory(&self, user_id: &str, channel_id: &str) -> Result<u64, StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute(
            "DELETE FROM bot_memory WHERE user_id = ?1 AND channel_id = ?2",
            params![user_id, channel_id],
        )?;
        Ok(deleted as u64)
    }

    async fn compact_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError> {
        let conn = self.conn.lock().await;
        // Delete the oldest entries beyond max_entries.
        // Keep the newest max_entries rows by deleting those NOT IN the top N.
        let deleted = conn.execute(
            "DELETE FROM bot_memory
             WHERE user_id = ?1 AND channel_id = ?2
               AND id NOT IN (
                   SELECT id FROM bot_memory
                   WHERE user_id = ?1 AND channel_id = ?2
                   ORDER BY created_at DESC, id DESC
                   LIMIT ?3
               )",
            params![user_id, channel_id, max_entries as i64],
        )?;
        Ok(deleted as u64)
    }

    // ── Bot Aliases ───────────────────────────────────────────

    async fn upsert_alias(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO bot_aliases (alias, target, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(alias) DO UPDATE SET
                target = excluded.target,
                created_by = excluded.created_by,
                created_at = excluded.created_at",
            params![alias, target, created_by, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    async fn list_aliases(&self) -> Result<Vec<(String, String)>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT alias, target FROM bot_aliases ORDER BY alias")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?;
        Ok(rows)
    }

    async fn delete_alias(&self, alias: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM bot_aliases WHERE alias = ?1", params![alias])?;
        Ok(())
    }

    // ── Audit Trail ───────────────────────────────────────────

    async fn insert_audit_entry(
        &self,
        entry: &crate::schema::audit::AuditEntry,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO audit_trail (id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id.to_string(),
                entry.timestamp.to_rfc3339(),
                entry.connector_name,
                entry.action_type,
                entry.action_summary,
                entry.verdict,
                entry.verdict_reason,
                entry.result,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn list_audit_entries(
        &self,
        filter: &crate::schema::audit::AuditFilter,
    ) -> Result<Vec<crate::schema::audit::AuditEntry>, StoreError> {
        let conn = self.conn.lock().await;

        let mut sql = String::from(
            "SELECT id, timestamp, connector_name, action_type, action_summary, verdict, verdict_reason, result FROM audit_trail WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref name) = filter.connector_name {
            sql.push_str(&format!(" AND connector_name = ?{param_idx}"));
            param_values.push(Box::new(name.clone()));
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
        if let Some(ref verdict) = filter.verdict {
            sql.push_str(&format!(" AND verdict = ?{param_idx}"));
            param_values.push(Box::new(verdict.clone()));
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
            let _ = param_idx;
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut entries = Vec::new();
        for (id_str, ts_str, connector, action_type, summary, verdict, reason, result) in rows {
            let id = uuid::Uuid::parse_str(&id_str)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            entries.push(crate::schema::audit::AuditEntry {
                id,
                timestamp,
                connector_name: connector,
                action_type,
                action_summary: summary,
                verdict,
                verdict_reason: reason,
                result,
            });
        }
        Ok(entries)
    }

    async fn export_audit(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<crate::schema::audit::AuditEntry>, StoreError> {
        self.list_audit_entries(&crate::schema::audit::AuditFilter {
            after: Some(*after),
            before: Some(*before),
            ..Default::default()
        })
        .await
    }

    async fn delete_audit_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute(
            "DELETE FROM audit_trail WHERE timestamp < ?1",
            params![before.to_rfc3339()],
        )?;
        Ok(deleted as u64)
    }

    async fn get_safety_config(&self) -> Result<Option<SafetyConfigRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT window_title, auto_lock_minutes, content_protected, quick_hide_shortcut, updated_at
             FROM safety_config WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        );

        match result {
            Ok((title, minutes, protected, shortcut, updated)) => {
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(SafetyConfigRow {
                    window_title: title,
                    auto_lock_minutes: minutes as u32,
                    content_protected: protected,
                    quick_hide_shortcut: shortcut,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn upsert_safety_config(&self, config: &SafetyConfigRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO safety_config (id, window_title, auto_lock_minutes, content_protected, quick_hide_shortcut, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                window_title = excluded.window_title,
                auto_lock_minutes = excluded.auto_lock_minutes,
                content_protected = excluded.content_protected,
                quick_hide_shortcut = excluded.quick_hide_shortcut,
                updated_at = excluded.updated_at",
            params![
                config.window_title,
                config.auto_lock_minutes as i64,
                config.content_protected,
                config.quick_hide_shortcut,
                config.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn insert_formation(&self, row: &FormationRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO formations (id, name, intent, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.id,
                row.name,
                row.intent,
                row.status,
                row.created_at.to_rfc3339(),
                row.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn list_formations(&self) -> Result<Vec<FormationRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, name, intent, status, created_at, updated_at FROM formations ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut formations = Vec::new();
        for row in rows {
            let (id, name, intent, status, created, updated) = row?;
            let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            formations.push(FormationRow { id, name, intent, status, created_at, updated_at });
        }
        Ok(formations)
    }

    async fn get_formation(&self, id: &str) -> Result<Option<FormationRow>, StoreError> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT id, name, intent, status, created_at, updated_at FROM formations WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        );

        match result {
            Ok((fid, name, intent, status, created, updated)) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&created)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                Ok(Some(FormationRow { id: fid, name, intent, status, created_at, updated_at }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn update_formation_status(&self, id: &str, status: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE formations SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )?;
        Ok(())
    }

    async fn delete_formation(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM formation_members WHERE formation_id = ?1", params![id])?;
        conn.execute("DELETE FROM formations WHERE id = ?1", params![id])?;
        Ok(())
    }

    async fn insert_formation_member(&self, row: &FormationMemberRow) -> Result<(), StoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO formation_members (id, formation_id, connector_name, role_hint)
             VALUES (?1, ?2, ?3, ?4)",
            params![row.id, row.formation_id, row.connector_name, row.role_hint],
        )?;
        Ok(())
    }

    async fn list_formation_members(&self, formation_id: &str) -> Result<Vec<FormationMemberRow>, StoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, formation_id, connector_name, role_hint FROM formation_members WHERE formation_id = ?1",
        )?;
        let rows = stmt.query_map(params![formation_id], |row| {
            Ok(FormationMemberRow {
                id: row.get(0)?,
                formation_id: row.get(1)?,
                connector_name: row.get(2)?,
                role_hint: row.get(3)?,
            })
        })?;

        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }
        Ok(members)
    }

    fn panic_wipe(&self) -> Result<(), StoreError> {
        // Close the connection to release file locks
        // (acquiring the mutex ensures no other operations are in progress)
        let _conn = self.conn.blocking_lock();

        // Wipe all SQLite files if we have a path
        if let Some(ref path) = self.path {
            super::wipe::secure_wipe_sqlite(path)?;
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

    // ── Bot Sessions ──────────────────────────────────────────

    fn test_session(user: &str, channel: &str) -> crate::schema::bot::SessionRow {
        crate::schema::bot::SessionRow {
            user_id: user.into(),
            channel_id: channel.into(),
            last_bot_message: None,
            pending_command: None,
            state_data: "{}".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get_session() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let session = test_session("user1", "chan1");
        store.upsert_session(&session).await.unwrap();

        let loaded = store.get_session("user1", "chan1").await.unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.user_id, "user1");
        assert_eq!(loaded.channel_id, "chan1");
    }

    #[tokio::test]
    async fn test_session_upsert_updates_existing() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let mut session = test_session("user1", "chan1");
        store.upsert_session(&session).await.unwrap();

        session.pending_command = Some("search".into());
        session.updated_at = Utc::now();
        store.upsert_session(&session).await.unwrap();

        let loaded = store.get_session("user1", "chan1").await.unwrap().unwrap();
        assert_eq!(loaded.pending_command.as_deref(), Some("search"));
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let result = store.get_session("nobody", "nowhere").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .upsert_session(&test_session("u1", "c1"))
            .await
            .unwrap();
        store.delete_session("u1", "c1").await.unwrap();
        assert!(store.get_session("u1", "c1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_session_isolation() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let mut s1 = test_session("user1", "chan1");
        s1.pending_command = Some("cmd1".into());
        let mut s2 = test_session("user2", "chan1");
        s2.pending_command = Some("cmd2".into());

        store.upsert_session(&s1).await.unwrap();
        store.upsert_session(&s2).await.unwrap();

        let loaded1 = store.get_session("user1", "chan1").await.unwrap().unwrap();
        let loaded2 = store.get_session("user2", "chan1").await.unwrap().unwrap();
        assert_eq!(loaded1.pending_command.as_deref(), Some("cmd1"));
        assert_eq!(loaded2.pending_command.as_deref(), Some("cmd2"));
    }

    // ── User Preferences ──────────────────────────────────────

    #[tokio::test]
    async fn test_upsert_and_get_user_prefs() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let prefs = crate::schema::bot::UserPrefsRow {
            user_id: "user1".into(),
            timezone: "America/New_York".into(),
            language: "en".into(),
            notifications_enabled: false,
            updated_at: Utc::now(),
        };
        store.upsert_user_prefs(&prefs).await.unwrap();

        let loaded = store.get_user_prefs("user1").await.unwrap().unwrap();
        assert_eq!(loaded.timezone, "America/New_York");
        assert!(!loaded.notifications_enabled);
    }

    #[tokio::test]
    async fn test_user_prefs_not_found() {
        let store = SqliteBackend::open_in_memory().unwrap();
        assert!(store.get_user_prefs("nobody").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_user_prefs_upsert_updates() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let prefs = crate::schema::bot::UserPrefsRow {
            user_id: "u1".into(),
            timezone: "UTC".into(),
            language: "en".into(),
            notifications_enabled: false,
            updated_at: Utc::now(),
        };
        store.upsert_user_prefs(&prefs).await.unwrap();

        let updated = crate::schema::bot::UserPrefsRow {
            timezone: "Europe/London".into(),
            notifications_enabled: true,
            updated_at: Utc::now(),
            ..prefs
        };
        store.upsert_user_prefs(&updated).await.unwrap();

        let loaded = store.get_user_prefs("u1").await.unwrap().unwrap();
        assert_eq!(loaded.timezone, "Europe/London");
        assert!(loaded.notifications_enabled);
    }

    // ── Bot Memory ────────────────────────────────────────────

    fn test_memory(user: &str, channel: &str, content: &[u8]) -> crate::schema::bot::MemoryRow {
        crate::schema::bot::MemoryRow {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.into(),
            channel_id: channel.into(),
            category: "conversation".into(),
            schema_version: 1,
            author: "user".into(),
            source: "user_input".into(),
            content_encrypted: content.to_vec(),
            nonce: vec![0u8; 24],
            content_hash: None,
            parent_id: None,
            trust_score: 1.0,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn test_insert_and_get_memory() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let entry = test_memory("u1", "c1", b"encrypted_data");
        store.insert_memory(&entry).await.unwrap();

        let entries = store.get_memory("u1", "c1", 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_encrypted, b"encrypted_data");
        assert_eq!(entries[0].author, "user");
        assert!((entries[0].trust_score - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_get_memory_respects_limit() {
        let store = SqliteBackend::open_in_memory().unwrap();
        for i in 0..5 {
            let mut entry = test_memory("u1", "c1", format!("msg{i}").as_bytes());
            entry.created_at = Utc::now() + chrono::Duration::seconds(i);
            store.insert_memory(&entry).await.unwrap();
        }

        let entries = store.get_memory("u1", "c1", 3).await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_memory() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .insert_memory(&test_memory("u1", "c1", b"a"))
            .await
            .unwrap();
        store
            .insert_memory(&test_memory("u1", "c1", b"b"))
            .await
            .unwrap();

        let deleted = store.delete_memory("u1", "c1").await.unwrap();
        assert_eq!(deleted, 2);
        assert!(store.get_memory("u1", "c1", 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_compact_memory() {
        let store = SqliteBackend::open_in_memory().unwrap();
        for i in 0..10 {
            let mut entry = test_memory("u1", "c1", format!("msg{i}").as_bytes());
            entry.created_at = Utc::now() + chrono::Duration::seconds(i);
            store.insert_memory(&entry).await.unwrap();
        }

        let deleted = store.compact_memory("u1", "c1", 3).await.unwrap();
        assert_eq!(deleted, 7);

        let remaining = store.get_memory("u1", "c1", 100).await.unwrap();
        assert_eq!(remaining.len(), 3);
    }

    #[tokio::test]
    async fn test_memory_isolation_across_users() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .insert_memory(&test_memory("u1", "c1", b"user1_data"))
            .await
            .unwrap();
        store
            .insert_memory(&test_memory("u2", "c1", b"user2_data"))
            .await
            .unwrap();

        let u1_entries = store.get_memory("u1", "c1", 10).await.unwrap();
        assert_eq!(u1_entries.len(), 1);
        assert_eq!(u1_entries[0].content_encrypted, b"user1_data");
    }

    // ── Bot Aliases ───────────────────────────────────────────

    #[tokio::test]
    async fn test_upsert_and_list_aliases() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store.upsert_alias("s", "search", "user1").await.unwrap();
        store.upsert_alias("g", "github", "user1").await.unwrap();

        let aliases = store.list_aliases().await.unwrap();
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0], ("g".into(), "github".into()));
        assert_eq!(aliases[1], ("s".into(), "search".into()));
    }

    #[tokio::test]
    async fn test_upsert_alias_updates_existing() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store.upsert_alias("s", "search", "user1").await.unwrap();
        store.upsert_alias("s", "status", "user2").await.unwrap();

        let aliases = store.list_aliases().await.unwrap();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0], ("s".into(), "status".into()));
    }

    #[tokio::test]
    async fn test_delete_alias() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store.upsert_alias("s", "search", "user1").await.unwrap();
        store.delete_alias("s").await.unwrap();
        assert!(store.list_aliases().await.unwrap().is_empty());
    }

    // ── Audit Trail ──────────────────────────────────────────

    fn test_audit_entry(connector: &str, verdict: &str) -> crate::schema::audit::AuditEntry {
        crate::schema::audit::AuditEntry {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            connector_name: connector.into(),
            action_type: "RunConnector".into(),
            action_summary: "test action".into(),
            verdict: verdict.into(),
            verdict_reason: String::new(),
            result: "ok".into(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_list_audit_entries() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .insert_audit_entry(&test_audit_entry("connector-test", "go"))
            .await
            .unwrap();
        store
            .insert_audit_entry(&test_audit_entry("connector-test", "throttle"))
            .await
            .unwrap();

        let entries = store
            .list_audit_entries(&crate::schema::audit::AuditFilter::default())
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_audit_filter_by_connector() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .insert_audit_entry(&test_audit_entry("connector-a", "go"))
            .await
            .unwrap();
        store
            .insert_audit_entry(&test_audit_entry("connector-b", "go"))
            .await
            .unwrap();

        let entries = store
            .list_audit_entries(&crate::schema::audit::AuditFilter {
                connector_name: Some("connector-a".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].connector_name, "connector-a");
    }

    #[tokio::test]
    async fn test_audit_filter_by_verdict() {
        let store = SqliteBackend::open_in_memory().unwrap();
        store
            .insert_audit_entry(&test_audit_entry("test", "go"))
            .await
            .unwrap();
        store
            .insert_audit_entry(&test_audit_entry("test", "throttle"))
            .await
            .unwrap();
        store
            .insert_audit_entry(&test_audit_entry("test", "pause"))
            .await
            .unwrap();

        let entries = store
            .list_audit_entries(&crate::schema::audit::AuditFilter {
                verdict: Some("throttle".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].verdict, "throttle");
    }

    #[tokio::test]
    async fn test_audit_delete_before() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let mut old = test_audit_entry("test", "go");
        old.timestamp = Utc::now() - chrono::Duration::days(30);
        store.insert_audit_entry(&old).await.unwrap();

        let new = test_audit_entry("test", "go");
        store.insert_audit_entry(&new).await.unwrap();

        let deleted = store
            .delete_audit_before(&(Utc::now() - chrono::Duration::days(7)))
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let remaining = store
            .list_audit_entries(&crate::schema::audit::AuditFilter::default())
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[tokio::test]
    async fn test_audit_export_time_range() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let entry = test_audit_entry("test", "go");
        store.insert_audit_entry(&entry).await.unwrap();

        let entries = store
            .export_audit(
                &(Utc::now() - chrono::Duration::hours(1)),
                &(Utc::now() + chrono::Duration::hours(1)),
            )
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
    }
}
