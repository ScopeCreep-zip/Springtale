//! Approval-queue operations.
//!
//! Both the pending queue and the resolve path go through the runtime's
//! `ApprovalGate` trait, so no surface builds its own decision plumbing.

use chrono::Utc;
use serde::Deserialize;
use thiserror::Error;

use crate::approval::{ApprovalDecision, ApprovalError, ApprovalRequest, ApprovalRequestId};
use crate::state::RuntimeState;

/// Fallback attribution label when the caller does not name an approver.
///
/// Every surface is already authenticated (bearer token / local vault),
/// so this is an audit-row label, not an authorisation decision.
pub const DEFAULT_APPROVER: &str = "maintainer";

/// Fallback reason recorded on a denial with no stated reason.
pub const DEFAULT_DENY_REASON: &str = "denied by maintainer";

/// The decision a caller landed on a pending request.
#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolveDecision {
    Approve,
    Deny,
}

/// Request body for resolving one pending approval.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct ResolveRequest {
    pub decision: ResolveDecision,
    #[serde(default)]
    pub approver: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Why a resolve could not land.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// No approval gate is wired (test build or boot-incomplete).
    #[error("no approval gate wired")]
    NoGate,
    /// The gate rejected the decision.
    #[error(transparent)]
    Gate(#[from] ApprovalError),
}

/// Outstanding approval requests.
///
/// No gate wired means no queue: an empty list, so every surface can
/// render uniformly instead of special-casing an error.
pub async fn pending(state: &RuntimeState) -> Vec<ApprovalRequest> {
    match state.capability_bridge.approval_gate() {
        Some(gate) => gate.pending().await,
        None => Vec::new(),
    }
}

/// Land a decision for one pending approval.
pub async fn resolve(
    state: &RuntimeState,
    id: ApprovalRequestId,
    req: ResolveRequest,
) -> Result<(), ResolveError> {
    let gate = state
        .capability_bridge
        .approval_gate()
        .ok_or(ResolveError::NoGate)?
        .clone();

    let decision = match req.decision {
        ResolveDecision::Approve => ApprovalDecision::Approved {
            approver: req.approver.unwrap_or_else(|| DEFAULT_APPROVER.to_owned()),
            approved_at: Utc::now(),
        },
        ResolveDecision::Deny => ApprovalDecision::Denied {
            reason: req.reason.unwrap_or_else(|| DEFAULT_DENY_REASON.to_owned()),
            denied_at: Utc::now(),
        },
    };

    gate.resolve(id, decision).await.map_err(ResolveError::Gate)
}
