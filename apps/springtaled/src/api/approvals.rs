//! Approval queue HTTP surface.
//!
//! Wired to `springtale-runtime::approval::DefaultDenyApprovalGate`.
//! The runtime's `CapabilityBridge::execute` fires the gate whenever a
//! connector that declared `Capability::ShellExec` is invoked
//! (OpenClaw CVE-2026-25253 1-click-RCE class — see Phase-7 audit
//! Finding A in `~/.claude/plans/mighty-honking-pinwheel.md`). The
//! requestor blocks until a `POST /approvals/:id` lands or the gate's
//! 60s deny-fallback fires.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use serde::Deserialize;

use springtale_runtime::{ApprovalDecision, ApprovalError, ApprovalRequestId};

use super::state::AppState;

/// `GET /approvals` — list outstanding approval requests.
///
/// The UI polls this (or subscribes via a future SSE channel) to
/// render the pending queue. Each entry includes the connector name,
/// the capability being requested, the human-readable summary, and
/// the request id needed to resolve.
pub async fn list_pending(State(state): State<AppState>) -> impl IntoResponse {
    let gate = match state.runtime.capability_bridge.approval_gate() {
        Some(g) => g.clone(),
        None => {
            // No gate wired = no pending queue. Return empty list
            // rather than erroring so the UI can render uniformly.
            return (StatusCode::OK, Json(serde_json::json!({ "pending": [] })));
        }
    };
    let pending = gate.pending().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "pending": pending })),
    )
}

/// Request body for `POST /approvals/{id}`.
///
/// `approver` defaults to `"maintainer"` if absent — the management
/// API is already HMAC-bearer-authenticated, so the caller has
/// proven they hold the vault passphrase; `approver` is only the
/// audit-row attribution label.
#[derive(Debug, Deserialize)]
pub struct ResolveBody {
    pub decision: ResolveDecision,
    #[serde(default)]
    pub approver: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveDecision {
    Approve,
    Deny,
}

/// `POST /approvals/{id}` — land a decision for a pending approval.
///
/// Status mapping:
/// - 200: decision recorded, requestor will see it.
/// - 404: no pending request with that id (timeout already fired, or
///   wrong id).
/// - 409: a prior decision already landed (idempotent — UI shouldn't
///   double-click).
/// - 503: no approval gate wired (test build or boot-incomplete).
pub async fn resolve(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResolveBody>,
) -> Result<impl IntoResponse, StatusCode> {
    let id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let id = ApprovalRequestId(id);

    let gate = state
        .runtime
        .capability_bridge
        .approval_gate()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
        .clone();

    let approver = body.approver.unwrap_or_else(|| "maintainer".to_owned());
    let decision = match body.decision {
        ResolveDecision::Approve => ApprovalDecision::Approved {
            approver,
            approved_at: Utc::now(),
        },
        ResolveDecision::Deny => ApprovalDecision::Denied {
            reason: body
                .reason
                .unwrap_or_else(|| "denied by maintainer".to_owned()),
            denied_at: Utc::now(),
        },
    };

    match gate.resolve(id, decision).await {
        Ok(()) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "resolved": true })),
        )),
        Err(ApprovalError::Unknown(_)) => Err(StatusCode::NOT_FOUND),
        Err(ApprovalError::DuplicateResolve(_)) => Err(StatusCode::CONFLICT),
        Err(ApprovalError::Shutdown) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}
