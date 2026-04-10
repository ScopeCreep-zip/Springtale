use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// POST /memory/audit
pub async fn audit_memory(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let result = operations::memory::audit_memory(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(result))
}

/// POST /memory/compact
pub async fn compact_memory(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let max_entries = body
        .get("max_entries")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as usize;
    operations::memory::compact_memory(&*state.runtime.store, max_entries)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "compacted": true })))
}
