mod aliases;
mod audit;
mod config;
mod connectors;
mod events;
mod formations;
mod jobs;
mod memory;
mod rules;
mod safety;
mod sessions;
mod wasm;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::error::StoreError;
use crate::migrations;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::execution::ExecutionResultRow;
use crate::schema::formations::{FormationMemberRow, FormationRow};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::safety::SafetyConfigRow;
use crate::schema::wasm::WasmBinaryRow;
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
        self.insert_rule_impl(rule).await
    }

    async fn find_rules_by_trigger(&self, trigger_type: &str) -> Result<Vec<Rule>, StoreError> {
        self.find_rules_by_trigger_impl(trigger_type).await
    }

    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError> {
        self.list_rules_impl().await
    }

    async fn toggle_rule(&self, id: &RuleId, enabled: bool) -> Result<(), StoreError> {
        self.toggle_rule_impl(id, enabled).await
    }

    async fn delete_rule(&self, id: &RuleId) -> Result<(), StoreError> {
        self.delete_rule_impl(id).await
    }

    // ── Connectors ─────────────────────────────────────────────

    async fn register_connector(&self, row: &ConnectorRow) -> Result<(), StoreError> {
        self.register_connector_impl(row).await
    }

    async fn list_connectors(&self) -> Result<Vec<ConnectorRow>, StoreError> {
        self.list_connectors_impl().await
    }

    async fn set_connector_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError> {
        self.set_connector_enabled_impl(name, enabled).await
    }

    async fn remove_connector(&self, name: &str) -> Result<(), StoreError> {
        self.remove_connector_impl(name).await
    }

    // ── Events ─────────────────────────────────────────────────

    async fn log_event(&self, event: &EventEntry) -> Result<(), StoreError> {
        self.log_event_impl(event).await
    }

    async fn list_events(&self, filter: &EventFilter) -> Result<Vec<EventEntry>, StoreError> {
        self.list_events_impl(filter).await
    }

    async fn delete_events_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        self.delete_events_before_impl(before).await
    }

    // ── Jobs ───────────────────────────────────────────────────

    async fn enqueue_job(&self, job: &JobRow) -> Result<JobId, StoreError> {
        self.enqueue_job_impl(job).await
    }

    async fn dequeue_job(&self) -> Result<Option<JobRow>, StoreError> {
        self.dequeue_job_impl().await
    }

    async fn complete_job(&self, id: &JobId) -> Result<(), StoreError> {
        self.complete_job_impl(id).await
    }

    async fn fail_job(&self, id: &JobId, error: &str) -> Result<(), StoreError> {
        self.fail_job_impl(id, error).await
    }

    // ── Bot Sessions ──────────────────────────────────────────

    async fn upsert_session(&self, session: &SessionRow) -> Result<(), StoreError> {
        self.upsert_session_impl(session).await
    }

    async fn get_session(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError> {
        self.get_session_impl(user_id, channel_id).await
    }

    async fn delete_session(&self, user_id: &str, channel_id: &str) -> Result<(), StoreError> {
        self.delete_session_impl(user_id, channel_id).await
    }

    async fn list_sessions(&self) -> Result<Vec<SessionRow>, StoreError> {
        self.list_sessions_impl().await
    }

    // ── User Preferences ──────────────────────────────────────

    async fn upsert_user_prefs(&self, prefs: &UserPrefsRow) -> Result<(), StoreError> {
        self.upsert_user_prefs_impl(prefs).await
    }

    async fn get_user_prefs(&self, user_id: &str) -> Result<Option<UserPrefsRow>, StoreError> {
        self.get_user_prefs_impl(user_id).await
    }

    // ── Bot Memory ────────────────────────────────────────────

    async fn insert_memory(&self, entry: &MemoryRow) -> Result<(), StoreError> {
        self.insert_memory_impl(entry).await
    }

    async fn get_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError> {
        self.get_memory_impl(user_id, channel_id, limit).await
    }

    async fn delete_memory(&self, user_id: &str, channel_id: &str) -> Result<u64, StoreError> {
        self.delete_memory_impl(user_id, channel_id).await
    }

    async fn compact_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError> {
        self.compact_memory_impl(user_id, channel_id, max_entries).await
    }

    // ── Bot Aliases ───────────────────────────────────────────

    async fn upsert_alias(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError> {
        self.upsert_alias_impl(alias, target, created_by).await
    }

    async fn list_aliases(&self) -> Result<Vec<(String, String)>, StoreError> {
        self.list_aliases_impl().await
    }

    async fn delete_alias(&self, alias: &str) -> Result<(), StoreError> {
        self.delete_alias_impl(alias).await
    }

    // ── Audit Trail ───────────────────────────────────────────

    async fn insert_audit_entry(&self, entry: &AuditEntry) -> Result<(), StoreError> {
        self.insert_audit_entry_impl(entry).await
    }

    async fn list_audit_entries(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        self.list_audit_entries_impl(filter).await
    }

    async fn export_audit(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        self.export_audit_impl(after, before).await
    }

    async fn delete_audit_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError> {
        self.delete_audit_before_impl(before).await
    }

    // ── Safety Config ──────────────────────────────────────────

    async fn get_safety_config(&self) -> Result<Option<SafetyConfigRow>, StoreError> {
        self.get_safety_config_impl().await
    }

    async fn upsert_safety_config(&self, config: &SafetyConfigRow) -> Result<(), StoreError> {
        self.upsert_safety_config_impl(config).await
    }

    // ── Formations ────────────────────────────────────────────

    async fn insert_formation(&self, row: &FormationRow) -> Result<(), StoreError> {
        self.insert_formation_impl(row).await
    }

    async fn list_formations(&self) -> Result<Vec<FormationRow>, StoreError> {
        self.list_formations_impl().await
    }

    async fn get_formation(&self, id: &str) -> Result<Option<FormationRow>, StoreError> {
        self.get_formation_impl(id).await
    }

    async fn update_formation_status(&self, id: &str, status: &str) -> Result<(), StoreError> {
        self.update_formation_status_impl(id, status).await
    }

    async fn update_formation_intent(&self, id: &str, intent: &str) -> Result<(), StoreError> {
        self.update_formation_intent_impl(id, intent).await
    }

    async fn delete_formation(&self, id: &str) -> Result<(), StoreError> {
        self.delete_formation_impl(id).await
    }

    async fn insert_formation_member(&self, row: &FormationMemberRow) -> Result<(), StoreError> {
        self.insert_formation_member_impl(row).await
    }

    async fn list_formation_members(
        &self,
        formation_id: &str,
    ) -> Result<Vec<FormationMemberRow>, StoreError> {
        self.list_formation_members_impl(formation_id).await
    }

    // ── Config Store ────────────────────────────────────────────

    async fn get_config(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.get_config_impl(key).await
    }

    async fn set_config(&self, key: &str, value_json: &str) -> Result<(), StoreError> {
        self.set_config_impl(key, value_json).await
    }

    async fn list_config(&self) -> Result<Vec<(String, String)>, StoreError> {
        self.list_config_impl().await
    }

    async fn delete_config(&self, key: &str) -> Result<(), StoreError> {
        self.delete_config_impl(key).await
    }

    // ── WASM Binaries ──────────────────────────────────────────

    async fn store_wasm_binary(
        &self,
        name: &str,
        wasm_bytes: &[u8],
        manifest_json: &str,
        wasm_hash: &str,
        author: &str,
    ) -> Result<(), StoreError> {
        self.store_wasm_binary_impl(name, wasm_bytes, manifest_json, wasm_hash, author)
            .await
    }

    async fn get_wasm_binary(
        &self,
        name: &str,
    ) -> Result<Option<WasmBinaryRow>, StoreError> {
        self.get_wasm_binary_impl(name).await
    }

    async fn list_wasm_binaries(&self) -> Result<Vec<WasmBinaryRow>, StoreError> {
        self.list_wasm_binaries_impl().await
    }

    async fn delete_wasm_binary(&self, name: &str) -> Result<(), StoreError> {
        self.delete_wasm_binary_impl(name).await
    }

    // ── Execution Results ──────────────────────────────────────

    async fn insert_execution_result(
        &self,
        id: &str,
        connector_name: &str,
        rule_id: Option<&str>,
        rule_name: Option<&str>,
        output_json: &str,
        success: bool,
        error_message: Option<&str>,
    ) -> Result<(), StoreError> {
        self.insert_execution_result_impl(
            id,
            connector_name,
            rule_id,
            rule_name,
            output_json,
            success,
            error_message,
        )
        .await
    }

    async fn list_execution_results(
        &self,
        connector_name: &str,
        limit: usize,
    ) -> Result<Vec<ExecutionResultRow>, StoreError> {
        self.list_execution_results_impl(connector_name, limit).await
    }

    // ── Emergency ─────────────────────────────────────────────

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
