use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

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

/// G5d — request body for toggling disguise-active.
#[derive(Deserialize)]
pub struct DisguiseActiveBody {
    pub active: bool,
}

/// POST /safety/disguise/active — focused endpoint that flips just
/// the disguise-active flag without re-sending the whole config.
/// Avoids the lost-update race two tabs would hit on the full-config
/// PUT path.
pub async fn set_disguise_active(
    State(state): State<AppState>,
    Json(body): Json<DisguiseActiveBody>,
) -> Result<impl IntoResponse, StatusCode> {
    let active = operations::safety::set_disguise_active(&state.runtime, body.active)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to set disguise active");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "disguise_active": active })),
    ))
}

/// G5d — request body for switching the disguise profile.
#[derive(Deserialize)]
pub struct DisguiseProfileBody {
    pub app_name: String,
    pub icon_id: String,
}

/// POST /safety/disguise/profile — atomic two-field update of which
/// disguise the app should display. Doesn't touch `disguise_active`.
pub async fn set_disguise_profile(
    State(state): State<AppState>,
    Json(body): Json<DisguiseProfileBody>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::safety::set_disguise_profile(&state.runtime, body.app_name, body.icon_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to set disguise profile");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "saved": true }))))
}

/// G5d — request body for the panic-tap threshold.
#[derive(Deserialize)]
pub struct PanicTapCountBody {
    pub count: u32,
}

/// POST /safety/panic_tap_count — update how many rapid title-bar
/// taps trigger panic-wipe. Server-bounded `[0, 10]`; values out of
/// range return 400 to prevent a survivor accidentally configuring
/// panic-wipe unreachable.
pub async fn set_panic_tap_count(
    State(state): State<AppState>,
    Json(body): Json<PanicTapCountBody>,
) -> Result<impl IntoResponse, StatusCode> {
    let count = operations::safety::set_panic_tap_count(&state.runtime, body.count)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "rejected panic_tap_count");
            StatusCode::BAD_REQUEST
        })?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "panic_tap_count": count })),
    ))
}
