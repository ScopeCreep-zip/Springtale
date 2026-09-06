mod ai_token_usage;
mod aliases;
mod api_tokens;
mod approvals;
mod audit;
mod config;
mod connectors;
mod cooperation;
mod dedupe;
mod events;
mod executions;
mod formations;
mod jobs;
mod memory;
mod mental_model_workspaces;
mod rules;
mod safety;
mod sessions;
mod wasm;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::error::StoreError;
use crate::schema;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::cooperation::{CoopCasOutcome, CoopDepositRow};
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::execution::ExecutionResultRow;
use crate::schema::formations::{FormationMemberRow, FormationRow};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::mental_model::MentalModelBundle;
use crate::schema::safety::SafetyConfigRow;
use crate::schema::wasm::WasmBinaryRow;
use springtale_core::rule::types::{Rule, RuleId};

/// SQLite-backed storage. Single-file, zero external dependencies.
///
/// Connection is wrapped in `Arc<std::sync::Mutex>` and all database
/// operations run inside `tokio::task::spawn_blocking` to avoid holding
/// async resources during synchronous `rusqlite` I/O. This follows
/// rusqlite maintainer guidance (issue #697).
pub struct SqliteBackend {
    conn: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl SqliteBackend {
    /// Open or create an **unencrypted** SQLite database at the given path.
    ///
    /// Test-only (plan 0.5): production stores are always encrypted, so
    /// this constructor exists for unit tests that need a file-backed
    /// store without a key. Use `open_encrypted` everywhere else.
    #[cfg(test)]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_with_key(path, None)
    }

    /// Open or create an encrypted SQLite database.
    ///
    /// Uses SQLite3MultipleCiphers (sqlite3mc) with ChaCha20-Poly1305.
    /// The encryption key should be a hex-encoded 32-byte key derived
    /// from the vault passphrase. Full-database encryption — same
    /// approach as Signal (SQLCipher) but with ChaCha20.
    pub fn open_encrypted(path: impl AsRef<Path>, hex_key: &str) -> Result<Self, StoreError> {
        Self::open_with_key(path, Some(hex_key))
    }

    fn open_with_key(path: impl AsRef<Path>, hex_key: Option<&str>) -> Result<Self, StoreError> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = open_and_configure(path, hex_key)?;

        // Opening never destroys a database. If the file does not
        // match the schema this build expects, `apply_schema` returns
        // `StoreError::SchemaVersion` and the file is left untouched.
        // Only `panic_wipe` may call `secure_wipe_sqlite`.
        schema::apply_schema(&conn)?;

        tracing::info!(path = %path.display(), "SQLite store opened");

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(path.to_owned()),
        })
    }

    /// Open an in-memory SQLite database (for testing).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        schema::apply_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        })
    }

    /// Get the database file path (None for in-memory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// Open a SQLite connection and apply per-process configuration:
/// secure file permissions, optional cipher key, and the standard
/// pragma set.
fn open_and_configure(path: &Path, hex_key: Option<&str>) -> Result<Connection, StoreError> {
    let conn = Connection::open(path)?;

    #[cfg(unix)]
    set_db_permissions(path)?;

    // sqlite3mc requires PRAGMA key before WAL mode or schema apply.
    // Vault already derives via Argon2id, so we use raw-key format
    // (x'hex') to skip the cipher's KDF pass.
    if let Some(key) = hex_key {
        conn.execute_batch("PRAGMA cipher = 'chacha20';")?;
        conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))?;
        tracing::info!("database encryption active (ChaCha20-Poly1305)");
    }

    configure_connection(&conn)?;
    Ok(conn)
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

    async fn set_rule_activation_error(
        &self,
        id: &RuleId,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        self.set_rule_activation_error_impl(id, error).await
    }

    async fn get_rule_activation_errors(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, StoreError> {
        self.get_rule_activation_errors_impl().await
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
        self.compact_memory_impl(user_id, channel_id, max_entries)
            .await
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

    async fn list_audit_chain(&self) -> Result<Vec<AuditEntry>, StoreError> {
        self.list_audit_chain_impl().await
    }

    // ── AI token usage ─────────────────────────────────────────

    async fn ai_token_usage_get(&self, agent_id: &str, day_ymd: u32) -> Result<u64, StoreError> {
        self.ai_token_usage_get_impl(agent_id, day_ymd).await
    }

    async fn ai_token_usage_set(
        &self,
        agent_id: &str,
        day_ymd: u32,
        tokens_used: u64,
    ) -> Result<(), StoreError> {
        self.ai_token_usage_set_impl(agent_id, day_ymd, tokens_used)
            .await
    }

    async fn ai_token_usage_reserve(
        &self,
        agent_id: &str,
        day_ymd: u32,
        requested: u64,
        limit: Option<u64>,
    ) -> Result<crate::backend::AiTokenReserveOutcome, StoreError> {
        self.ai_token_usage_reserve_impl(agent_id, day_ymd, requested, limit)
            .await
    }

    async fn ai_token_usage_commit(
        &self,
        agent_id: &str,
        day_ymd: u32,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), StoreError> {
        self.ai_token_usage_commit_impl(agent_id, day_ymd, prior_reservation, actual_tokens)
            .await
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

    async fn delete_formation_member(
        &self,
        formation_id: &str,
        connector_name: &str,
    ) -> Result<(), StoreError> {
        self.delete_formation_member_impl(formation_id, connector_name)
            .await
    }

    async fn get_formation_momentum(
        &self,
        formation_id: &str,
    ) -> Result<Option<crate::schema::formations::FormationMomentumRow>, StoreError> {
        self.get_formation_momentum_impl(formation_id).await
    }

    async fn upsert_formation_momentum(
        &self,
        row: &crate::schema::formations::FormationMomentumRow,
    ) -> Result<(), StoreError> {
        self.upsert_formation_momentum_impl(row).await
    }

    async fn get_formation_rally(
        &self,
        formation_id: &str,
    ) -> Result<Option<crate::schema::formations::FormationRallyRow>, StoreError> {
        self.get_formation_rally_impl(formation_id).await
    }

    async fn upsert_formation_rally(
        &self,
        row: &crate::schema::formations::FormationRallyRow,
    ) -> Result<(), StoreError> {
        self.upsert_formation_rally_impl(row).await
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
        author_pubkey_hex: &str,
        manifest_sig_hex: &str,
    ) -> Result<(), StoreError> {
        self.store_wasm_binary_impl(
            name,
            wasm_bytes,
            manifest_json,
            wasm_hash,
            author,
            author_pubkey_hex,
            manifest_sig_hex,
        )
        .await
    }

    async fn get_wasm_binary(&self, name: &str) -> Result<Option<WasmBinaryRow>, StoreError> {
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
        input: &crate::schema::execution::ExecutionResultInput<'_>,
    ) -> Result<(), StoreError> {
        self.insert_execution_result_impl(input).await
    }

    async fn list_execution_results(
        &self,
        connector_name: &str,
        limit: usize,
    ) -> Result<Vec<ExecutionResultRow>, StoreError> {
        self.list_execution_results_impl(connector_name, limit)
            .await
    }

    // ── Cooperation: Atomic CAS (§13) ─────────────────────────

    async fn coop_cas_write(
        &self,
        tick: i64,
        writer: &str,
        key: &str,
        expected: Option<&[u8]>,
        proposed: &[u8],
    ) -> Result<CoopCasOutcome, StoreError> {
        self.coop_cas_write_impl(
            tick,
            writer.to_owned(),
            key.to_owned(),
            expected.map(|b| b.to_vec()),
            proposed.to_vec(),
        )
        .await
    }

    // ── Cooperation: Environment-Mediated Handoff (§20) ───────

    async fn coop_deposit(
        &self,
        location: &str,
        payload: &[u8],
        depositor: &str,
        ttl_secs: Option<i64>,
    ) -> Result<(), StoreError> {
        self.coop_deposit_impl(
            location.to_owned(),
            payload.to_vec(),
            depositor.to_owned(),
            ttl_secs,
        )
        .await
    }

    async fn coop_collect(
        &self,
        location: &str,
        collector: &str,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.coop_collect_impl(location.to_owned(), collector.to_owned())
            .await
    }

    async fn coop_sweep_expired(&self) -> Result<u64, StoreError> {
        self.coop_sweep_expired_impl().await
    }

    async fn coop_list_deposits(&self) -> Result<Vec<CoopDepositRow>, StoreError> {
        self.coop_list_deposits_impl().await
    }

    // ── Cooperation: Shared Mental Model (§21) ────────────────

    async fn mental_model_save(
        &self,
        formation_id: &str,
        bundle: &MentalModelBundle,
    ) -> Result<(), StoreError> {
        self.mental_model_save_impl(formation_id.to_owned(), bundle.clone())
            .await
    }

    async fn mental_model_load(&self, formation_id: &str) -> Result<MentalModelBundle, StoreError> {
        self.mental_model_load_impl(formation_id.to_owned()).await
    }

    async fn mental_model_clear(&self, formation_id: &str) -> Result<(), StoreError> {
        self.mental_model_clear_impl(formation_id.to_owned()).await
    }

    // ── Mental Model: External-Workspace Directory (D1) ────────

    async fn mental_model_workspace_upsert(
        &self,
        formation_id: &str,
        row: &crate::schema::mental_model::MentalModelWorkspaceRow,
    ) -> Result<(), StoreError> {
        self.mental_model_workspace_upsert_impl(formation_id.to_owned(), row.clone())
            .await
    }

    async fn mental_model_workspaces_for_formation(
        &self,
        formation_id: &str,
        connector_filter: Option<&str>,
    ) -> Result<Vec<crate::schema::mental_model::MentalModelWorkspaceRow>, StoreError> {
        self.mental_model_workspaces_for_formation_impl(
            formation_id.to_owned(),
            connector_filter.map(str::to_owned),
        )
        .await
    }

    async fn mental_model_workspace_delete(
        &self,
        formation_id: &str,
        workspace_key: &str,
    ) -> Result<(), StoreError> {
        self.mental_model_workspace_delete_impl(formation_id.to_owned(), workspace_key.to_owned())
            .await
    }

    async fn mental_model_workspace_touch(
        &self,
        formation_id: &str,
        workspace_key: &str,
        now_unix_ms: i64,
    ) -> Result<(), StoreError> {
        self.mental_model_workspace_touch_impl(
            formation_id.to_owned(),
            workspace_key.to_owned(),
            now_unix_ms,
        )
        .await
    }

    // ── Dedupe (Phase A) ──────────────────────────────────────

    async fn dedupe_check(
        &self,
        formation_id: Option<&str>,
        rule_id: &str,
        bucket: &str,
        key_hash: &str,
        history: u32,
    ) -> Result<crate::schema::dedupe::DedupeOutcome, StoreError> {
        self.dedupe_check_impl(
            formation_id.map(str::to_owned),
            rule_id.to_owned(),
            bucket.to_owned(),
            key_hash.to_owned(),
            history,
        )
        .await
    }

    // ── Approval-over-chat (W2) ───────────────────────────────

    async fn insert_pending_approval(
        &self,
        row: crate::schema::approvals::PendingApprovalRow,
    ) -> Result<(), StoreError> {
        self.insert_pending_approval_impl(row).await
    }

    async fn get_pending_approval(
        &self,
        id: &str,
    ) -> Result<Option<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        self.get_pending_approval_impl(id.to_owned()).await
    }

    async fn resolve_pending_approval(
        &self,
        id: &str,
        decision_json: &str,
    ) -> Result<bool, StoreError> {
        self.resolve_pending_approval_impl(id.to_owned(), decision_json.to_owned())
            .await
    }

    async fn list_pending_approvals(
        &self,
        now_ms: i64,
    ) -> Result<Vec<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        self.list_pending_approvals_impl(now_ms).await
    }

    async fn expire_pending_approvals(
        &self,
        now_ms: i64,
    ) -> Result<Vec<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        self.expire_pending_approvals_impl(now_ms).await
    }

    // ── Long-lived API tokens (6.6) ───────────────────────────

    async fn insert_api_token(
        &self,
        row: crate::schema::api_tokens::ApiTokenRow,
    ) -> Result<(), StoreError> {
        self.insert_api_token_impl(row).await
    }

    async fn list_api_tokens(
        &self,
    ) -> Result<Vec<crate::schema::api_tokens::ApiTokenRow>, StoreError> {
        self.list_api_tokens_impl().await
    }

    async fn find_api_token_by_hash(
        &self,
        hash: &[u8],
    ) -> Result<Option<crate::schema::api_tokens::ApiTokenRow>, StoreError> {
        self.find_api_token_by_hash_impl(hash.to_vec()).await
    }

    async fn touch_api_token(&self, id: &str, now_ms: i64) -> Result<(), StoreError> {
        self.touch_api_token_impl(id.to_owned(), now_ms).await
    }

    async fn delete_api_token(&self, id: &str) -> Result<bool, StoreError> {
        self.delete_api_token_impl(id.to_owned()).await
    }

    async fn upsert_tool_loop_checkpoint(
        &self,
        row: crate::schema::approvals::ToolLoopCheckpointRow,
    ) -> Result<(), StoreError> {
        self.upsert_tool_loop_checkpoint_impl(row).await
    }

    async fn get_tool_loop_checkpoint(
        &self,
        session_key: &str,
    ) -> Result<Option<crate::schema::approvals::ToolLoopCheckpointRow>, StoreError> {
        self.get_tool_loop_checkpoint_impl(session_key.to_owned(), false)
            .await
    }

    async fn get_checkpoint_by_approval(
        &self,
        approval_id: &str,
    ) -> Result<Option<crate::schema::approvals::ToolLoopCheckpointRow>, StoreError> {
        self.get_tool_loop_checkpoint_impl(approval_id.to_owned(), true)
            .await
    }

    async fn delete_tool_loop_checkpoint(&self, session_key: &str) -> Result<(), StoreError> {
        self.delete_tool_loop_checkpoint_impl(session_key.to_owned())
            .await
    }

    async fn list_tool_loop_checkpoints(
        &self,
    ) -> Result<Vec<crate::schema::approvals::ToolLoopCheckpointRow>, StoreError> {
        self.list_tool_loop_checkpoints_impl().await
    }

    async fn find_approval_by_summary(
        &self,
        summary: &str,
        since_ms: i64,
    ) -> Result<Option<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        self.find_approval_by_summary_impl(summary.to_owned(), since_ms)
            .await
    }

    // ── Executions log (Phase B) ──────────────────────────────

    async fn record_execution_start(
        &self,
        exec: crate::schema::executions::ExecutionRow,
    ) -> Result<(), StoreError> {
        self.record_execution_start_impl(exec).await
    }

    async fn record_execution_finish(
        &self,
        execution_id: &str,
        status: crate::schema::executions::ExecutionStatus,
        error_kind: Option<&str>,
        finished_at: i64,
    ) -> Result<(), StoreError> {
        self.record_execution_finish_impl(
            execution_id.to_owned(),
            status,
            error_kind.map(str::to_owned),
            finished_at,
        )
        .await
    }

    async fn record_execution_step(
        &self,
        step: crate::schema::executions::ExecutionStepRow,
    ) -> Result<(), StoreError> {
        self.record_execution_step_impl(step).await
    }

    async fn list_executions(
        &self,
        filter: crate::schema::executions::ExecutionFilter,
    ) -> Result<Vec<crate::schema::executions::ExecutionSummary>, StoreError> {
        self.list_executions_impl(filter).await
    }

    async fn get_execution_steps(
        &self,
        execution_id: &str,
    ) -> Result<Vec<crate::schema::executions::ExecutionStepRow>, StoreError> {
        self.get_execution_steps_impl(execution_id.to_owned()).await
    }

    async fn vacuum_executions(&self, now_ms: i64) -> Result<u64, StoreError> {
        self.vacuum_executions_impl(now_ms).await
    }

    // ── Emergency ─────────────────────────────────────────────

    fn panic_wipe(&self) -> Result<(), StoreError> {
        // Close the connection to release file locks
        // (acquiring the mutex ensures no other operations are in progress)
        let _conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::Database("lock poisoned".into()))?;

        // Wipe all SQLite files if we have a path
        if let Some(ref path) = self.path {
            super::wipe::secure_wipe_sqlite(path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
