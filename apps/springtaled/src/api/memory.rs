use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// POST /memory/audit
#[utoipa::path(
    post, operation_id = "memory_audit_memory",
    path = "/memory/audit",
    tag = "memory",
    responses((status = 200, description = "Memory audit report", body = Object))
)]
pub async fn audit_memory(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let result = operations::memory::audit_memory(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(result))
}

/// POST /memory/compact
#[utoipa::path(
    post, operation_id = "memory_compact_memory",
    path = "/memory/compact",
    tag = "memory",
    request_body = operations::memory::CompactMemoryRequest,
    responses((status = 200, description = "Memory compacted", body = Object))
)]
pub async fn compact_memory(
    State(state): State<AppState>,
    Json(req): Json<operations::memory::CompactMemoryRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let deleted = operations::memory::compact_memory(&*state.runtime.store, req.max_entries)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({ "compacted": true, "deleted": deleted }),
    ))
}
