use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// GET /sessions — list all active bot sessions.
///
/// Shows per-user, per-channel conversation state. Useful for
/// monitoring which automated conversations are in progress.
#[utoipa::path(
    get, operation_id = "sessions_list",
    path = "/sessions",
    tag = "sessions",
    responses((status = 200, description = "Active bot sessions", body = Vec<Object>))
)]
pub async fn list(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let sessions = operations::sessions::list(&*state.runtime.store)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list sessions");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
}
