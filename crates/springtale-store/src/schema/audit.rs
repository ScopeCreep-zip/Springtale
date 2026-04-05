use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Row type for the `audit_trail` table (append-only sentinel log).
///
/// Every action dispatched through the system is logged here with
/// the sentinel's verdict. The table is append-only by convention —
/// sentinel never issues UPDATE or DELETE against it.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
