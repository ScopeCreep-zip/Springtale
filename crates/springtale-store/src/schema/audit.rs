use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Row type for the `audit_trail` table (append-only + tamper-evident).
///
/// Every action dispatched through the system is logged here with
/// the sentinel's verdict. The table is append-only by convention
/// AND tamper-evident via the SHA-256 row-hash chain (Phase-7 audit
/// Finding B). Sentinel never issues UPDATE or DELETE against it;
/// the verifier in `audit_chain` walks rows in `chain_seq` order at
/// daemon startup and fails closed on any mismatch.
///
/// Chain construction: `row_hash = SHA-256(prev_hash ||
/// canonical_row_json)` where `canonical_row_json` is the
/// sorted-key JSON of every field EXCEPT the chain columns
/// themselves. Genesis anchor (`prev_hash` of the first row) is the
/// SHA-256 hex of the vault identity key's public bytes — so the
/// chain is bound to the vault, not just the SQLite file.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AuditEntry {
    /// Unique entry identifier.
    pub id: Uuid,
    /// When the action was evaluated.
    pub timestamp: DateTime<Utc>,
    /// Which connector was involved.
    pub connector_name: String,
    /// Action type (e.g., "RunConnector", "SendMessage").
    pub action_type: String,
    /// Human-readable summary of the action.
    pub action_summary: String,
    /// Sentinel verdict: "go", "throttle", "pause", "quarantine".
    pub verdict: String,
    /// Reason for non-go verdicts.
    pub verdict_reason: String,
    /// Action result (success/failure message).
    pub result: String,
    /// SHA-256 hex of the previous row's `row_hash` (or the vault
    /// genesis anchor for the first row). 64 hex chars. Empty on
    /// pre-v6-migration rows.
    #[serde(default)]
    pub prev_hash: String,
    /// SHA-256 hex of `prev_hash || canonical_row_json`. 64 hex
    /// chars. Empty on pre-v6-migration rows.
    #[serde(default)]
    pub row_hash: String,
    /// Monotonic insert order — the verifier walks ascending.
    #[serde(default)]
    pub chain_seq: i64,
}

/// Filter parameters for querying the audit trail.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Filter by connector name.
    pub connector_name: Option<String>,
    /// Return entries after this time.
    pub after: Option<DateTime<Utc>>,
    /// Return entries before this time.
    pub before: Option<DateTime<Utc>>,
    /// Filter by verdict.
    pub verdict: Option<String>,
    /// Maximum number of entries to return.
    pub limit: Option<u32>,
    /// Number of entries to skip (for pagination).
    pub offset: Option<u32>,
}
