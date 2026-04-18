use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::execution::ExecutionResultRow;
use crate::schema::formations::{
    FormationMemberRow, FormationMomentumRow, FormationRallyRow, FormationRow,
};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::safety::SafetyConfigRow;
use springtale_core::rule::types::{Rule, RuleId};

/// Persistence backend for all Springtale data.
///
/// Phase 1a: SQLite (single file, zero external dependencies).
/// All crates access persistence through this trait. No raw SQL
/// outside the store crate.
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    // ── Rules ──────────────────────────────────────────────────

    /// Insert a new rule. Returns the assigned RuleId.
    async fn insert_rule(&self, rule: &Rule) -> Result<RuleId, StoreError>;

    /// Find all rules that match a given trigger type.
    async fn find_rules_by_trigger(&self, trigger_type: &str) -> Result<Vec<Rule>, StoreError>;

    /// List all rules.
    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError>;

    /// Toggle a rule's status (enabled/disabled).
    async fn toggle_rule(&self, id: &RuleId, enabled: bool) -> Result<(), StoreError>;

    /// Delete a rule.
    async fn delete_rule(&self, id: &RuleId) -> Result<(), StoreError>;

    /// Persist or clear a rule's activation error.
    /// Called by TriggerRegistry on attach success (None) or failure (Some).
    async fn set_rule_activation_error(
        &self,
        id: &RuleId,
        error: Option<&str>,
    ) -> Result<(), StoreError>;

    /// Get activation errors for all rules that have one.
    /// Returns a map of rule ID → error message.
    async fn get_rule_activation_errors(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, StoreError>;

    // ── Connectors ─────────────────────────────────────────────

    /// Register a connector's manifest.
    async fn register_connector(&self, row: &ConnectorRow) -> Result<(), StoreError>;

    /// List all registered connectors.
    async fn list_connectors(&self) -> Result<Vec<ConnectorRow>, StoreError>;

    /// Enable or disable a connector.
    async fn set_connector_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError>;

    /// Remove a connector.
    async fn remove_connector(&self, name: &str) -> Result<(), StoreError>;

    // ── Events ─────────────────────────────────────────────────

    /// Log an event (audit trail entry).
    async fn log_event(&self, event: &EventEntry) -> Result<(), StoreError>;

    /// List events matching a filter.
    async fn list_events(&self, filter: &EventFilter) -> Result<Vec<EventEntry>, StoreError>;

    /// Delete events older than a given timestamp (retention enforcement).
    async fn delete_events_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError>;

    // ── Jobs ───────────────────────────────────────────────────

    /// Enqueue a new job.
    async fn enqueue_job(&self, job: &JobRow) -> Result<JobId, StoreError>;

    /// Dequeue the next pending job (marks it as running).
    async fn dequeue_job(&self) -> Result<Option<JobRow>, StoreError>;

    /// Mark a job as complete.
    async fn complete_job(&self, id: &JobId) -> Result<(), StoreError>;

    /// Mark a job as failed with an error message.
    async fn fail_job(&self, id: &JobId, error: &str) -> Result<(), StoreError>;

    // ── Bot Sessions ──────────────────────────────────────────

    /// Upsert a bot session (insert or update on conflict).
    async fn upsert_session(&self, session: &SessionRow) -> Result<(), StoreError>;

    /// Get a bot session by (user_id, channel_id).
    async fn get_session(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError>;

    /// Delete a bot session.
    async fn delete_session(&self, user_id: &str, channel_id: &str) -> Result<(), StoreError>;

    /// List all active bot sessions.
    async fn list_sessions(&self) -> Result<Vec<SessionRow>, StoreError>;

    // ── User Preferences ──────────────────────────────────────

    /// Upsert user preferences.
    async fn upsert_user_prefs(&self, prefs: &UserPrefsRow) -> Result<(), StoreError>;

    /// Get user preferences by user_id.
    async fn get_user_prefs(&self, user_id: &str) -> Result<Option<UserPrefsRow>, StoreError>;

    // ── Bot Memory ────────────────────────────────────────────

    /// Store an encrypted memory entry.
    async fn insert_memory(&self, entry: &MemoryRow) -> Result<(), StoreError>;

    /// Get recent memory entries for a (user_id, channel_id),
    /// ordered by created_at DESC.
    async fn get_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError>;

    /// Delete all memory entries for a (user_id, channel_id).
    /// Returns the number of entries deleted.
    async fn delete_memory(&self, user_id: &str, channel_id: &str) -> Result<u64, StoreError>;

    /// Delete the oldest entries beyond `max_entries` for a
    /// (user_id, channel_id). Used by compaction.
    /// Returns the number of entries deleted.
    async fn compact_memory(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError>;

    // ── Bot Aliases ───────────────────────────────────────────

    /// Upsert a command alias.
    async fn upsert_alias(
        &self,
        alias: &str,
        target: &str,
        created_by: &str,
    ) -> Result<(), StoreError>;

    /// List all command aliases as (alias, target) pairs.
    async fn list_aliases(&self) -> Result<Vec<(String, String)>, StoreError>;

    /// Delete a command alias.
    async fn delete_alias(&self, alias: &str) -> Result<(), StoreError>;

    // ── Audit Trail ───────────────────────────────────────────

    /// Insert an audit trail entry (append-only).
    async fn insert_audit_entry(&self, entry: &AuditEntry) -> Result<(), StoreError>;

    /// List audit trail entries matching a filter.
    async fn list_audit_entries(&self, filter: &AuditFilter)
    -> Result<Vec<AuditEntry>, StoreError>;

    /// Export audit trail entries within a time range.
    async fn export_audit(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<AuditEntry>, StoreError>;

    /// Delete audit entries older than a given timestamp (retention).
    async fn delete_audit_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError>;

    // ── Safety Config ──────────────────────────────────────────

    /// Get the current safety configuration.
    /// Returns None if no config has been saved yet (use defaults).
    async fn get_safety_config(&self) -> Result<Option<SafetyConfigRow>, StoreError> {
        Ok(None)
    }

    /// Upsert safety configuration (single-row table).
    async fn upsert_safety_config(&self, _config: &SafetyConfigRow) -> Result<(), StoreError> {
        Ok(())
    }

    // ── Formations ────────────────────────────────────────────

    /// Insert a new formation.
    async fn insert_formation(&self, _row: &FormationRow) -> Result<(), StoreError> {
        Ok(())
    }

    /// List all formations.
    async fn list_formations(&self) -> Result<Vec<FormationRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Get a formation by ID.
    async fn get_formation(&self, _id: &str) -> Result<Option<FormationRow>, StoreError> {
        Ok(None)
    }

    /// Update a formation's status.
    async fn update_formation_status(&self, _id: &str, _status: &str) -> Result<(), StoreError> {
        Ok(())
    }

    /// Update a formation's intent.
    async fn update_formation_intent(&self, _id: &str, _intent: &str) -> Result<(), StoreError> {
        Ok(())
    }

    /// Delete a formation and its members.
    async fn delete_formation(&self, _id: &str) -> Result<(), StoreError> {
        Ok(())
    }

    /// Insert a formation member.
    async fn insert_formation_member(&self, _row: &FormationMemberRow) -> Result<(), StoreError> {
        Ok(())
    }

    /// List members of a formation.
    async fn list_formation_members(
        &self,
        _formation_id: &str,
    ) -> Result<Vec<FormationMemberRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Delete a single formation member by formation ID and connector name.
    async fn delete_formation_member(
        &self,
        _formation_id: &str,
        _connector_name: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    // ── Formation Cooperation State ──────────────────────────────

    /// Get momentum state for a formation.
    async fn get_formation_momentum(
        &self,
        _formation_id: &str,
    ) -> Result<Option<FormationMomentumRow>, StoreError> {
        Ok(None)
    }

    /// Insert or update momentum state for a formation.
    async fn upsert_formation_momentum(
        &self,
        _row: &FormationMomentumRow,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Get rally state for a formation.
    async fn get_formation_rally(
        &self,
        _formation_id: &str,
    ) -> Result<Option<FormationRallyRow>, StoreError> {
        Ok(None)
    }

    /// Insert or update rally state for a formation.
    async fn upsert_formation_rally(&self, _row: &FormationRallyRow) -> Result<(), StoreError> {
        Ok(())
    }

    // ── Config Store ────────────────────────────────────────────

    /// Get a config value by key.
    async fn get_config(&self, _key: &str) -> Result<Option<String>, StoreError> {
        Ok(None)
    }

    /// Set a config value (upsert).
    async fn set_config(&self, _key: &str, _value_json: &str) -> Result<(), StoreError> {
        Ok(())
    }

    /// List all config entries.
    async fn list_config(&self) -> Result<Vec<(String, String)>, StoreError> {
        Ok(Vec::new())
    }

    /// Delete a config entry by key.
    async fn delete_config(&self, _key: &str) -> Result<(), StoreError> {
        Ok(())
    }

    // ── WASM Binaries ──────────────────────────────────────────

    /// Store a WASM connector binary for persistence across restarts.
    async fn store_wasm_binary(
        &self,
        _name: &str,
        _wasm_bytes: &[u8],
        _manifest_json: &str,
        _wasm_hash: &str,
        _author: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Retrieve a WASM binary by connector name.
    async fn get_wasm_binary(
        &self,
        _name: &str,
    ) -> Result<Option<crate::schema::wasm::WasmBinaryRow>, StoreError> {
        Ok(None)
    }

    /// List all persisted WASM binaries.
    async fn list_wasm_binaries(
        &self,
    ) -> Result<Vec<crate::schema::wasm::WasmBinaryRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Delete a persisted WASM binary.
    async fn delete_wasm_binary(&self, _name: &str) -> Result<(), StoreError> {
        Ok(())
    }

    // ── Execution Results ──────────────────────────────────────

    /// Store an execution result (output data from a rule/action execution).
    async fn insert_execution_result(
        &self,
        _input: &crate::schema::execution::ExecutionResultInput<'_>,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// List recent execution results for a connector.
    /// Returns: (id, connector_name, rule_name, output_json, success, error_message, created_at)
    async fn list_execution_results(
        &self,
        _connector_name: &str,
        _limit: usize,
    ) -> Result<Vec<ExecutionResultRow>, StoreError> {
        Ok(Vec::new())
    }

    // ── Emergency ─────────────────────────────────────────────

    /// Emergency data destruction. Must complete within 3 seconds.
    ///
    /// Overwrites all persistent data with random bytes and deletes files.
    /// For in-memory backends, clears all data structures.
    ///
    /// Not async — the 3-second deadline cannot afford async overhead.
    /// Default implementation is a no-op (safe for backends with no persistence).
    fn panic_wipe(&self) -> Result<(), StoreError> {
        Ok(())
    }
}
