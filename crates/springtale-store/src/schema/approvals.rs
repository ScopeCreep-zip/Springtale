//! Approval-over-chat schema — durable pending approvals + tool-loop
//! checkpoints (W2).
//!
//! Backs `springtale-runtime`'s `ChatApprovalGate`: a pending row exists
//! while a destructive action awaits the owner's decision; the checkpoint
//! row lets a paused chat tool loop survive a daemon restart (2026 HITL
//! interrupt pattern — pause = persisted state + stable thread id).

use serde::{Deserialize, Serialize};

/// One pending (or just-decided) approval. `decision_json` is `None`
/// while waiting; rows past `expires_at` are denied on read/boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApprovalRow {
    /// ApprovalRequestId UUID string.
    pub id: String,
    pub connector_name: String,
    /// Serialized `Capability` (JSON) — store stays decoupled from the
    /// connector crate's types.
    pub capability_json: String,
    /// Firing bot id; `None` for chat-direct paths.
    pub agent_id: Option<String>,
    /// Human-readable card body (action + arg summary, never raw argv).
    pub summary: String,
    /// Unix ms.
    pub requested_at: i64,
    /// Unix ms — deny after this instant (deny-by-default posture).
    pub expires_at: i64,
    /// Serialized `ApprovalDecision` once resolved.
    pub decision_json: Option<String>,
}

/// Persisted chat tool-loop state while it blocks on one approval.
/// Keyed by SESSION (thread id, LangGraph-pattern durable resume);
/// `approval_id` correlates to the surviving pending approval, when any.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLoopCheckpointRow {
    /// Thread id (PRIMARY KEY) — the chat session to resume into.
    pub session_key: String,
    /// Correlation → `pending_approvals.id` (None once decoupled).
    pub approval_id: Option<String>,
    /// Where to deliver the eventual result.
    pub origin_connector: String,
    pub origin_channel: String,
    /// Serialized `Vec<ChatMessage>` at pause time.
    pub messages_json: String,
    /// Serialized pending tool call awaiting the verdict.
    pub pending_tool_json: String,
    /// Unix ms.
    pub created_at: i64,
}
