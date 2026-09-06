use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use springtale_runtime::operations;

use super::state::AppState;

/// GET /safety — get the current safety configuration.
#[utoipa::path(
    get, operation_id = "safety_get_config",
    path = "/safety",
    tag = "safety",
    responses((status = 200, description = "Safety config", body = Object))
)]
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
#[utoipa::path(
    put, operation_id = "safety_save_config",
    path = "/safety",
    tag = "safety",
    request_body = Object,
    responses((status = 200, description = "Safety config saved", body = Object))
)]
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
#[derive(Deserialize, utoipa::ToSchema)]
pub struct DisguiseActiveBody {
    pub active: bool,
}

/// POST /safety/disguise/active — focused endpoint that flips just
/// the disguise-active flag without re-sending the whole config.
/// Avoids the lost-update race two tabs would hit on the full-config
/// PUT path.
#[utoipa::path(
    post, operation_id = "safety_set_disguise_active",
    path = "/safety/disguise/active",
    tag = "safety",
    request_body = DisguiseActiveBody,
    responses((status = 200, description = "Disguise flag flipped", body = Object))
)]
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
#[derive(Deserialize, utoipa::ToSchema)]
pub struct DisguiseProfileBody {
    pub app_name: String,
    pub icon_id: String,
}

/// POST /safety/disguise/profile — atomic two-field update of which
/// disguise the app should display. Doesn't touch `disguise_active`.
#[utoipa::path(
    post, operation_id = "safety_set_disguise_profile",
    path = "/safety/disguise/profile",
    tag = "safety",
    request_body = DisguiseProfileBody,
    responses((status = 200, description = "Disguise profile switched", body = Object))
)]
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
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PanicTapCountBody {
    pub count: u32,
}

/// POST /safety/panic_tap_count — update how many rapid title-bar
/// taps trigger panic-wipe. Server-bounded `[0, 10]`; values out of
/// range return 400 to prevent a survivor accidentally configuring
/// panic-wipe unreachable.
#[utoipa::path(
    post, operation_id = "safety_set_panic_tap_count",
    path = "/safety/panic_tap_count",
    tag = "safety",
    request_body = PanicTapCountBody,
    responses((status = 200, description = "Panic tap threshold saved", body = Object))
)]
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

/// POST /safety/panic-wipe — irreversibly destroy every row of local data.
///
/// Plan 2.1: the desktop used to reach `operations::safety::panic_wipe`
/// through its own in-process runtime. The daemon owns the store now, so
/// the route has to exist here or the button is dead on both shells.
#[utoipa::path(
    post, operation_id = "safety_panic_wipe",
    path = "/safety/panic-wipe",
    tag = "safety",
    responses((status = 200, description = "Panic wipe executed", body = Object))
)]
pub async fn panic_wipe(State(state): State<AppState>) -> impl IntoResponse {
    match operations::safety::panic_wipe(state.runtime.store.as_ref()).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "wiped": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// Body for both travel routes.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct TravelBody {
    /// Vault passphrase — encrypts (prepare) or decrypts (restore) the backup.
    pub passphrase: String,
    /// Absolute path of the encrypted backup file.
    pub backup_path: String,
}

/// POST /travel/prepare — write an encrypted backup, then wipe local data.
///
/// Per `ARCHITECTURE.md` §2.6 border-crossing mode. The caller is expected
/// to stop the daemon afterwards; unlike the old desktop command this does
/// not call `std::process::exit`, because the daemon may be serving other
/// clients and killing it out from under them is the wrong shutdown path.
#[utoipa::path(
    post, operation_id = "safety_travel_prepare",
    path = "/travel/prepare",
    tag = "safety",
    request_body = TravelBody,
    responses((status = 200, description = "Travel backup written", body = Object))
)]
pub async fn travel_prepare(
    State(state): State<AppState>,
    Json(body): Json<TravelBody>,
) -> impl IntoResponse {
    let store = std::sync::Arc::clone(&state.runtime.store);
    let result = tokio::task::spawn_blocking(move || {
        operations::travel::prepare(
            &springtale_store::paths::default_vault_path(),
            &springtale_store::paths::default_db_path(),
            &springtale_store::paths::default_config_path(),
            std::path::Path::new(&body.backup_path),
            body.passphrase.as_bytes(),
            store.as_ref(),
        )
        .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(serde_json::json!({ "prepared": true })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /travel/restore — decrypt a backup back into vault, database, config.
///
/// The daemon must be restarted afterwards: it is still holding the handles
/// to the files that were just replaced underneath it.
#[utoipa::path(
    post, operation_id = "safety_travel_restore",
    path = "/travel/restore",
    tag = "safety",
    request_body = TravelBody,
    responses((status = 200, description = "Travel backup restored", body = Object))
)]
pub async fn travel_restore(Json(body): Json<TravelBody>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || {
        operations::travel::restore(
            std::path::Path::new(&body.backup_path),
            &springtale_store::paths::default_vault_path(),
            &springtale_store::paths::default_db_path(),
            &springtale_store::paths::default_config_path(),
            body.passphrase.as_bytes(),
        )
        .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(serde_json::json!({ "restored": true })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
