use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::state::AppState;

/// GET /bot/status — runtime status summary.
pub async fn status(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let registry = state.runtime.registry.read().await;
    let connector_count = registry.list().len();
    drop(registry);

    let engine = state.runtime.engine.read().await;
    let rule_count = engine.list_rules().len();
    drop(engine);

    let formation_count = state
        .runtime
        .store
        .list_formations()
        .await
        .map(|f| f.len())
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "running": true,
        "connectors": connector_count,
        "rules": rule_count,
        "formations": formation_count,
    })))
}

/// GET /bot/formations — active formations with member info.
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
pub async fn memory(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    // List active sessions — metadata only, no decrypted content
    let sessions = state
        .runtime
        .store
        .list_sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // For each session, count memory entries without reading content
    let mut session_summaries = Vec::with_capacity(sessions.len());
    for session in &sessions {
        let entries = state
            .runtime
            .store
            .get_memory(&session.user_id, &session.channel_id, 1)
            .await
            .unwrap_or_default();

        session_summaries.push(serde_json::json!({
            "user_id": session.user_id,
            "channel_id": session.channel_id,
            "last_active": session.updated_at.to_rfc3339(),
            "has_entries": !entries.is_empty(),
        }));
    }

    Ok(Json(serde_json::json!({ "sessions": session_summaries })))
}

/// GET /bot/settings — persona, context window, tool policy (plan 6.3).
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
