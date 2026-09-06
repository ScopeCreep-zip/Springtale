use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Outcome of `StorageBackend::ai_token_usage_reserve`. Separated
/// from `QuotaCheck` (in `springtale-ai`) so the store crate stays
/// independent of the ai crate — the runtime layer converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiTokenReserveOutcome {
    Reserved { total_after: u64 },
    Denied { used: u64, limit: u64 },
}

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
use crate::schema::connectors::ConnectorRow;
use crate::schema::cooperation::{CoopCasOutcome, CoopDepositRow};
use crate::schema::dedupe::DedupeOutcome;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::execution::ExecutionResultRow;
use crate::schema::executions::{
    ExecutionFilter, ExecutionRow, ExecutionStatus, ExecutionStepRow, ExecutionSummary,
};
use crate::schema::formations::{
    FormationMemberRow, FormationMomentumRow, FormationRallyRow, FormationRow,
};
use crate::schema::jobs::{JobId, JobRow};
use crate::schema::mental_model::{MentalModelBundle, MentalModelWorkspaceRow};
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

    /// Walk the audit trail in `chain_seq` ascending order. Used by
    /// the daemon-startup verifier to recompute and check the SHA-256
    /// row-hash chain (Phase-7 audit Finding B). Default impl falls
    /// back to a `list_audit_entries` call sorted client-side — every
    /// backend should override with an indexed walk.
    async fn list_audit_chain(&self) -> Result<Vec<AuditEntry>, StoreError> {
        let mut rows = self
            .list_audit_entries(&AuditFilter {
                limit: Some(u32::MAX),
                ..Default::default()
            })
            .await?;
        rows.sort_by_key(|e| e.chain_seq);
        Ok(rows)
    }

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
    ///
    /// `author_pubkey_hex` / `manifest_sig_hex` are persisted out of
    /// band from the manifest body so the boot re-verifier can pin the
    /// install-time trust anchor (Phase-7 audit Finding #1, R-004).
    /// Callers MUST pass the install-time pubkey + signature; pre-
    /// migration rows that lack them carry empty strings and the
    /// verifier logs a warning rather than failing closed.
    async fn store_wasm_binary(
        &self,
        _name: &str,
        _wasm_bytes: &[u8],
        _manifest_json: &str,
        _wasm_hash: &str,
        _author: &str,
        _author_pubkey_hex: &str,
        _manifest_sig_hex: &str,
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

    // ── Cooperation: Atomic CAS (§13) ─────────────────────────
    //
    // Backs the interference detector's write-conflict classification.
    // Mirrors sled::Tree::compare_and_swap semantics via SQLite
    // BEGIN IMMEDIATE transactions.

    /// Attempt an atomic compare-and-swap write. `expected == None` means
    /// "key must not exist"; `Some(bytes)` means "current value must match".
    /// Returns `Applied` on success or `Mismatch` with the conflicting
    /// writer's current state for interference classification.
    async fn coop_cas_write(
        &self,
        _tick: i64,
        _writer: &str,
        _key: &str,
        _expected: Option<&[u8]>,
        _proposed: &[u8],
    ) -> Result<CoopCasOutcome, StoreError> {
        Ok(CoopCasOutcome::Applied)
    }

    // ── Cooperation: Environment-Mediated Handoff (§20) ───────
    //
    // Backs HandoffType::EnvironmentMediated with durable deposit + TTL
    // sweep, per Divinity/MH "leave-a-thing-for-your-partner" pattern.

    /// Deposit a payload at a location with optional TTL (seconds).
    /// Replaces any existing deposit at the same location (last-write-wins).
    async fn coop_deposit(
        &self,
        _location: &str,
        _payload: &[u8],
        _depositor: &str,
        _ttl_secs: Option<i64>,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Atomically claim and remove a deposit. Returns None if the location
    /// is empty or already claimed. UPDATE ... RETURNING guarantees
    /// exactly-once collection even under concurrent collectors.
    async fn coop_collect(
        &self,
        _location: &str,
        _collector: &str,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(None)
    }

    /// Delete deposits whose `expires_at` is before the current Unix time.
    /// Returns the number of rows swept. Called on a timer from runtime init.
    async fn coop_sweep_expired(&self) -> Result<u64, StoreError> {
        Ok(0)
    }

    /// List all active deposits (for observability / canvas UI). Surfaces
    /// depositor + deposited_at so the "handoff_ready" surface can display
    /// who handed off what and when. Per spec §20.3.
    async fn coop_list_deposits(&self) -> Result<Vec<CoopDepositRow>, StoreError> {
        Ok(Vec::new())
    }

    // ── Cooperation: Shared Mental Model (§21) ────────────────
    //
    // Five tables, saved/loaded as one transactional bundle per formation.
    // Callers are responsible for converting SharedMentalModel ↔ bundle
    // (conversion lives in the cooperation crate to keep store domain-agnostic).

    /// Save the full mental-model bundle for a formation. Replaces any
    /// existing rows for this formation (transactional delete + insert).
    async fn mental_model_save(
        &self,
        _formation_id: &str,
        _bundle: &MentalModelBundle,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Load the full mental-model bundle for a formation. Empty bundle
    /// if the formation has no stored state yet.
    async fn mental_model_load(
        &self,
        _formation_id: &str,
    ) -> Result<MentalModelBundle, StoreError> {
        Ok(MentalModelBundle::default())
    }

    /// Delete all mental-model rows for a formation.
    async fn mental_model_clear(&self, _formation_id: &str) -> Result<(), StoreError> {
        Ok(())
    }

    // ── External-workspace directory (D1) ─────────────────────
    //
    // Per-key upsert (not bundle-snapshot like the other mental_model_*
    // methods) because destinations arrive one at a time — passive
    // harvest fires on each inbound event, scan returns a few at a
    // time, and gossip merges are per-key. Bundle-snapshot semantics
    // would force every harvest to read the whole formation's
    // directory, conflict-resolve every entry, and write back —
    // wasteful when 99% of the time we're just touching one row.

    /// Insert or update one workspace entry. Implementations should
    /// use SQLite's `INSERT … ON CONFLICT(formation_id, workspace_key)
    /// DO UPDATE` semantics. Caller has already done gossip-delta
    /// merge resolution (see
    /// `springtale-cooperation::mental_model::external_workspaces::merge_gossip_delta`).
    async fn mental_model_workspace_upsert(
        &self,
        _formation_id: &str,
        _row: &MentalModelWorkspaceRow,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// List every workspace entry for a formation, newest-first.
    /// `connector_filter` narrows to one connector when set — the
    /// recipe deploy form's dropdown uses this to show only
    /// destinations matching the recipe's connector hint.
    async fn mental_model_workspaces_for_formation(
        &self,
        _formation_id: &str,
        _connector_filter: Option<&str>,
    ) -> Result<Vec<MentalModelWorkspaceRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Delete one workspace entry. The user explicitly removed a
    /// destination from the dropdown; the harvester won't recreate
    /// it until the connector emits another event mentioning the
    /// key.
    async fn mental_model_workspace_delete(
        &self,
        _formation_id: &str,
        _workspace_key: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Touch `last_seen_at` for one workspace without changing any
    /// other field. Called by the harvester when re-observing a
    /// destination we already know about — keeps the dropdown
    /// sorted by recency without rewriting the whole row.
    async fn mental_model_workspace_touch(
        &self,
        _formation_id: &str,
        _workspace_key: &str,
        _now_unix_ms: i64,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    // ── AI token usage (Phase-7 audit Finding D) ───────────────
    //
    // Per-bot daily quota persistence. The runtime's
    // `SqliteTokenQuota` calls these methods from inside
    // `check_and_reserve` and `commit`; the rows survive daemon
    // restart so quota counters don't reset.

    /// Read the tokens_used counter for `(agent_id, day_ymd)`.
    /// Returns `0` when no row exists for that bot/day (the row is
    /// created lazily on the first reservation). Default impl
    /// returns 0 — backends without quota persistence fall through
    /// to the in-process behaviour.
    async fn ai_token_usage_get(&self, _agent_id: &str, _day_ymd: u32) -> Result<u64, StoreError> {
        Ok(0)
    }

    /// Atomic reserve: read tokens_used, compare against limit, and
    /// if (tokens_used + requested) ≤ limit (or limit is None) write
    /// the new total back. Returns the post-reservation total on
    /// success, or the current tokens_used when the reservation
    /// would exceed limit. The caller passes `None` for limit to
    /// disable enforcement (records usage without denying).
    async fn ai_token_usage_reserve(
        &self,
        agent_id: &str,
        day_ymd: u32,
        requested: u64,
        limit: Option<u64>,
    ) -> Result<AiTokenReserveOutcome, StoreError> {
        let used = self.ai_token_usage_get(agent_id, day_ymd).await?;
        let new_total = used.saturating_add(requested);
        if let Some(cap) = limit
            && new_total > cap
        {
            return Ok(AiTokenReserveOutcome::Denied { used, limit: cap });
        }
        self.ai_token_usage_set(agent_id, day_ymd, new_total)
            .await?;
        Ok(AiTokenReserveOutcome::Reserved {
            total_after: new_total,
        })
    }

    /// Set the absolute tokens_used value for `(agent_id, day_ymd)`.
    /// Used by tests/admin to reset a counter. UPSERT semantics —
    /// creates the row if missing. Avoid for commit paths; use
    /// `ai_token_usage_commit` instead so the read-adjust-write runs
    /// atomically under the backend's lock.
    async fn ai_token_usage_set(
        &self,
        _agent_id: &str,
        _day_ymd: u32,
        _tokens_used: u64,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Atomically adjust the tokens_used counter from a pessimistic
    /// reservation to actual usage. Runs under the same backend lock
    /// as `ai_token_usage_reserve` so two concurrent commits for the
    /// same `(agent_id, day_ymd)` can't race a stale `used` into the
    /// write. Equivalent to
    /// `tokens_used = max(0, tokens_used - prior + actual)`.
    async fn ai_token_usage_commit(
        &self,
        agent_id: &str,
        day_ymd: u32,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), StoreError> {
        // Conservative default for backends that don't override:
        // round-trip via get+set. Production backends MUST override
        // for atomicity.
        let used = self.ai_token_usage_get(agent_id, day_ymd).await?;
        let adjusted = used
            .saturating_sub(prior_reservation)
            .saturating_add(actual_tokens);
        self.ai_token_usage_set(agent_id, day_ymd, adjusted).await
    }

    // ── Dedupe (Phase A) ───────────────────────────────────────

    /// Check whether `key_hash` has been seen for
    /// `(formation_id, rule_id, bucket)`. Atomic check-and-record:
    /// on `DedupeOutcome::Fresh` the hash is now recorded so the next
    /// call with the same key returns `SeenBefore`.
    ///
    /// `history` caps the entries retained per bucket (LRU prune).
    /// The dispatcher calls this from `Action::Dedupe`; chains
    /// short-circuit via `ChainError::Suppressed` on `SeenBefore`.
    ///
    /// Default impl is the conservative "always Fresh" — backends
    /// without dedupe persistence (in-memory, mock) keep the chain
    /// flowing rather than blocking it.
    async fn dedupe_check(
        &self,
        _formation_id: Option<&str>,
        _rule_id: &str,
        _bucket: &str,
        _key_hash: &str,
        _history: u32,
    ) -> Result<DedupeOutcome, StoreError> {
        Ok(DedupeOutcome::Fresh)
    }

    // ── Approval-over-chat (W2) ────────────────────────────────

    /// Persist a pending approval (ChatApprovalGate). Durable so the
    /// request survives restart. Default no-op keeps in-memory/mock
    /// backends working; SQLite overrides.
    async fn insert_pending_approval(
        &self,
        _row: crate::schema::approvals::PendingApprovalRow,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Fetch one approval row (pending or decided).
    async fn get_pending_approval(
        &self,
        _id: &str,
    ) -> Result<Option<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        Ok(None)
    }

    /// Land a decision (serialized `ApprovalDecision`) on a pending row.
    /// Returns `false` when the row is missing or already decided —
    /// the gate maps that to `DuplicateResolve` idempotency.
    async fn resolve_pending_approval(
        &self,
        _id: &str,
        _decision_json: &str,
    ) -> Result<bool, StoreError> {
        Ok(false)
    }

    /// Undecided, unexpired approvals (the owner's open card queue).
    async fn list_pending_approvals(
        &self,
        _now_ms: i64,
    ) -> Result<Vec<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Mark every undecided row past its expiry as denied (deny-by-
    /// default). Returns the rows so the boot sweep can notify/clean
    /// their checkpoints.
    async fn expire_pending_approvals(
        &self,
        _now_ms: i64,
    ) -> Result<Vec<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        Ok(Vec::new())
    }

    // ── Long-lived API tokens (6.6) ────────────────────────────
    //
    // A token is stored ONLY as `sha256(token)`; the backend can verify
    // a presented bearer but can never produce one. The default impls
    // are the deny-everything shape: a backend that does not implement
    // them accepts no long-lived token at all.

    /// Persist a freshly minted long-lived token (hash + name).
    async fn insert_api_token(
        &self,
        _row: crate::schema::api_tokens::ApiTokenRow,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Every long-lived token, newest first. Callers must never leak
    /// `token_hash` past the API boundary.
    async fn list_api_tokens(
        &self,
    ) -> Result<Vec<crate::schema::api_tokens::ApiTokenRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Look one up by `sha256(token)`. `None` means "never issued, or
    /// revoked" — both are a 401.
    async fn find_api_token_by_hash(
        &self,
        _hash: &[u8],
    ) -> Result<Option<crate::schema::api_tokens::ApiTokenRow>, StoreError> {
        Ok(None)
    }

    /// Record the last accepted request against a token (unix ms).
    async fn touch_api_token(&self, _id: &str, _now_ms: i64) -> Result<(), StoreError> {
        Ok(())
    }

    /// Revoke. `false` when the id was already gone.
    async fn delete_api_token(&self, _id: &str) -> Result<bool, StoreError> {
        Ok(false)
    }

    /// Persist the chat tool-loop state paused behind an approval.
    async fn upsert_tool_loop_checkpoint(
        &self,
        _row: crate::schema::approvals::ToolLoopCheckpointRow,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Fetch the checkpoint for one chat session (thread id), if any.
    async fn get_tool_loop_checkpoint(
        &self,
        _session_key: &str,
    ) -> Result<Option<crate::schema::approvals::ToolLoopCheckpointRow>, StoreError> {
        Ok(None)
    }

    /// Fetch the checkpoint correlated to one pending approval — the boot
    /// resumer's join from a surviving approval back to its conversation.
    async fn get_checkpoint_by_approval(
        &self,
        _approval_id: &str,
    ) -> Result<Option<crate::schema::approvals::ToolLoopCheckpointRow>, StoreError> {
        Ok(None)
    }

    /// Drop a session's checkpoint after the loop resumed (or was denied).
    async fn delete_tool_loop_checkpoint(&self, _session_key: &str) -> Result<(), StoreError> {
        Ok(())
    }

    /// All persisted paused threads — the boot resumer's worklist.
    async fn list_tool_loop_checkpoints(
        &self,
    ) -> Result<Vec<crate::schema::approvals::ToolLoopCheckpointRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Latest approval row (pending OR decided) whose bound-action summary
    /// matches, requested at/after `since_ms`. The boot resumer's join from
    /// an orphaned checkpoint back to its approval verdict.
    async fn find_approval_by_summary(
        &self,
        _summary: &str,
        _since_ms: i64,
    ) -> Result<Option<crate::schema::approvals::PendingApprovalRow>, StoreError> {
        Ok(None)
    }

    // ── Executions log (Phase B) ───────────────────────────────

    /// Record the start of a chain dispatch. Inserts one row into
    /// `executions` with `status = "running"` and the supplied
    /// metadata.
    ///
    /// Default impl is a no-op so the in-memory backend keeps
    /// working in tests — the SQLite backend overrides.
    async fn record_execution_start(&self, _exec: ExecutionRow) -> Result<(), StoreError> {
        Ok(())
    }

    /// Finalize a chain dispatch. Sets `status`, `error_kind`,
    /// `finished_at`, and `duration_ms` on the existing row.
    async fn record_execution_finish(
        &self,
        _execution_id: &str,
        _status: ExecutionStatus,
        _error_kind: Option<&str>,
        _finished_at: i64,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Append one step row to `execution_steps`. Sizes-only per
    /// the privacy default — content blob refs are populated by
    /// the recorder when `bot.retain_step_content` is on.
    async fn record_execution_step(&self, _step: ExecutionStepRow) -> Result<(), StoreError> {
        Ok(())
    }

    /// List recent executions filtered by `filter`. Newest-first;
    /// caller paginates with `filter.before` cursor.
    async fn list_executions(
        &self,
        _filter: ExecutionFilter,
    ) -> Result<Vec<ExecutionSummary>, StoreError> {
        Ok(Vec::new())
    }

    /// Return every step recorded for a single execution, ordered
    /// by `step_index`.
    async fn get_execution_steps(
        &self,
        _execution_id: &str,
    ) -> Result<Vec<ExecutionStepRow>, StoreError> {
        Ok(Vec::new())
    }

    /// Delete every executions row whose `retention_until` is at
    /// or before `now_ms`. Cascade deletes step rows via the
    /// `ON DELETE CASCADE` foreign key.
    ///
    /// Returns the number of executions purged so the background
    /// vacuum task can emit useful telemetry.
    async fn vacuum_executions(&self, _now_ms: i64) -> Result<u64, StoreError> {
        Ok(0)
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
