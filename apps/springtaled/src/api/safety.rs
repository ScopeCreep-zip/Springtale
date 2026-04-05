use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// GET /safety — get the current safety configuration.
pub async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    match operations::safety::get_safety_config(&state.runtime).await {
        Ok(config) => (StatusCode::OK, Json(serde_json::json!(config))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// PUT /safety — save the safety configuration.
pub async fn save_config(
    State(state): State<AppState>,
    Json(config): Json<springtale_store::SafetyConfigRow>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::safety::save_safety_config(&state.runtime, config)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to save safety config");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "saved": true }))))
}
