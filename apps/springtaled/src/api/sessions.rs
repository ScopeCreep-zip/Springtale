use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;

use super::state::AppState;

/// GET /sessions — list all active bot sessions.
///
/// Shows per-user, per-channel conversation state. Useful for
/// monitoring which automated conversations are in progress.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    match state.runtime.store.list_sessions().await {
        Ok(sessions) => {
            let entries: Vec<serde_json::Value> = sessions
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "user_id": s.user_id,
                        "channel_id": s.channel_id,
                        "last_bot_message": s.last_bot_message,
                        "pending_command": s.pending_command,
                        "state_data": s.state_data,
                        "created_at": s.created_at.to_rfc3339(),
                        "updated_at": s.updated_at.to_rfc3339(),
                    })
                })
                .collect();
            Json(serde_json::json!({ "sessions": entries }))
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to list sessions");
            Json(serde_json::json!({ "sessions": [], "error": e.to_string() }))
        }
    }
}
