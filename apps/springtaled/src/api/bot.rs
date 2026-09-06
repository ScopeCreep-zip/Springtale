use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::state::AppState;

/// GET /bot/status — runtime status summary.
#[utoipa::path(
    get, operation_id = "bot_status",
    path = "/bot/status",
    tag = "bot",
    responses((status = 200, description = "Bot runtime status", body = Object))
)]
pub async fn status(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let status = springtale_runtime::operations::bot::status(&state.runtime)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(status))
}

/// GET /bot/formations — active formations with member info.
#[utoipa::path(
    get, operation_id = "bot_formations",
    path = "/bot/formations",
    tag = "bot",
    responses((status = 200, description = "Formations the bot belongs to", body = Vec<Object>))
)]
pub async fn formations(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let formations = springtale_runtime::operations::formations::list_formations(&state.runtime)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "formations": formations })))
}

/// GET /bot/memory — list conversation session metadata (zero-knowledge).
///
/// Per zero-knowledge architecture pattern (Bitwarden, Signal, Matrix):
/// the server never exposes decrypted conversation content. Memory is
/// encrypted with XChaCha20-Poly1305 and the decryption key lives in
/// the bot runtime — not the API layer.
///
/// This endpoint returns session metadata only:
/// - user_id, channel_id (who)
/// - last_active (when)
/// - entry_count (how much)
///
/// Admin can see what sessions exist and how active they are without
/// reading message content. This protects vulnerable users' conversations
/// from API-level data exfiltration, even if the API token is compromised.
///
/// Sources:
/// - Bitwarden: "systems have no knowledge of, way to retrieve" encrypted content
/// - Matrix Synapse: "encrypted rooms, messages are encrypted, but not their metadata"
/// - Signal: separates delivery metadata from encrypted contents
#[utoipa::path(
    get, operation_id = "bot_memory",
    path = "/bot/memory",
    tag = "bot",
    responses((status = 200, description = "Session memory summary", body = Object))
)]
pub async fn memory(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let sessions = springtale_runtime::operations::bot::memory_summary(&state.runtime)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

/// GET /bot/settings — persona, context window, tool policy (plan 6.3).
#[utoipa::path(
    get, operation_id = "bot_get_settings",
    path = "/bot/settings",
    tag = "bot",
    responses((status = 200, description = "Current bot settings", body = springtale_runtime::operations::bot_settings::BotSettings))
)]
pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    match springtale_runtime::operations::bot_settings::get(&*state.runtime.store).await {
        Ok(settings) => (StatusCode::OK, Json(serde_json::json!(settings))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// PUT /bot/settings — replace them. Every literal tool in the allow-list
/// is checked against the connector registry, so a typo is a 400 rather
/// than a silently tool-less AI.
#[utoipa::path(
    put, operation_id = "bot_put_settings",
    path = "/bot/settings",
    tag = "bot",
    request_body = springtale_runtime::operations::bot_settings::BotSettings,
    responses((status = 200, description = "Settings saved", body = Object))
)]
pub async fn put_settings(
    State(state): State<AppState>,
    Json(settings): Json<springtale_runtime::operations::bot_settings::BotSettings>,
) -> impl IntoResponse {
    match springtale_runtime::operations::bot_settings::set(&state.runtime, settings).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "saved": true }))),
        Err(e @ springtale_runtime::OperationError::Validation(_)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
        Err(e) => {
            tracing::error!(error = %e, "failed to save bot settings");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        }
    }
}
