//! Approval queue HTTP surface.
//!
//! Wired to `springtale-runtime::approval::DefaultDenyApprovalGate`.
//! The runtime's `CapabilityBridge::execute` fires the gate whenever a
//! connector that declared `Capability::ShellExec` is invoked
//! (OpenClaw CVE-2026-25253 1-click-RCE class). The requestor blocks
//! until a `POST /approvals/:id` lands or the gate's 60s deny-fallback
//! fires. Queue and decision plumbing both live in
//! `operations::approvals`; this module only maps to status codes.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::ApprovalError;
use springtale_runtime::approval::ApprovalRequestId;
use springtale_runtime::operations::approvals::{self, ResolveError, ResolveRequest};

use super::state::AppState;

/// `GET /approvals` — list outstanding approval requests.
#[utoipa::path(
    get, operation_id = "approvals_list_pending",
    path = "/approvals",
    tag = "approvals",
    responses((status = 200, description = "Pending approval requests", body = Vec<Object>))
)]
pub async fn list_pending(State(state): State<AppState>) -> impl IntoResponse {
    let pending = approvals::pending(&state.runtime).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "pending": pending })),
    )
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
#[utoipa::path(
    post, operation_id = "approvals_resolve",
    path = "/approvals/{id}",
    tag = "approvals",
    params(("id" = String, Path, description = "Approval request id")),
    request_body = ResolveRequest,
    responses((status = 200, description = "Approval resolved", body = Object))
)]
pub async fn resolve(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResolveRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let id = ApprovalRequestId(
        id.parse::<uuid::Uuid>()
            .map_err(|_| StatusCode::BAD_REQUEST)?,
    );

    match approvals::resolve(&state.runtime, id, body).await {
        Ok(()) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "resolved": true })),
        )),
        Err(ResolveError::NoGate) => Err(StatusCode::SERVICE_UNAVAILABLE),
        Err(ResolveError::Gate(ApprovalError::Unknown(_))) => Err(StatusCode::NOT_FOUND),
        Err(ResolveError::Gate(ApprovalError::DuplicateResolve(_))) => Err(StatusCode::CONFLICT),
        Err(ResolveError::Gate(ApprovalError::Shutdown)) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}
