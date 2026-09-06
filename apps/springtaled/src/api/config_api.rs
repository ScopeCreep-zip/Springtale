use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /config/heartbeat
#[utoipa::path(
    get, operation_id = "config_api_get_heartbeat",
    path = "/config/heartbeat",
    tag = "config",
    responses((status = 200, description = "Heartbeat monitor state", body = operations::heartbeat::HeartbeatStatus))
)]
pub async fn get_heartbeat(State(state): State<AppState>) -> impl IntoResponse {
    Json(operations::heartbeat::get(&state.heartbeat_monitor).await)
}

/// PUT /config/heartbeat — persist the interval and apply it to the monitor.
#[utoipa::path(
    put, operation_id = "config_api_set_heartbeat",
    path = "/config/heartbeat",
    tag = "config",
    request_body = operations::heartbeat::SetHeartbeatRequest,
    responses((status = 200, description = "Heartbeat interval applied", body = operations::heartbeat::HeartbeatStatus))
)]
pub async fn set_heartbeat(
    State(state): State<AppState>,
    Json(req): Json<operations::heartbeat::SetHeartbeatRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let status =
        operations::heartbeat::set(&state.runtime, &state.heartbeat_monitor, req.interval_secs)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to set heartbeat interval");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    Ok(Json(status))
}

/// GET /config — list all config entries.
#[utoipa::path(
    get, operation_id = "config_api_list_config",
    path = "/config",
    tag = "config",
    responses((status = 200, description = "All config entries", body = Vec<Object>))
)]
pub async fn list_config(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let entries = operations::config::list_config(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "config": entries })))
}

/// GET /config/:key
#[utoipa::path(
    get, operation_id = "config_api_get_config",
    path = "/config/{key}",
    tag = "config",
    params(("key" = String, Path, description = "Config key")),
    responses((status = 200, description = "Config value", body = Object))
)]
pub async fn get_config(
    State(state): State<AppState>,
    ValidatedPath(key): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let value = operations::config::get_config(&*state.runtime.store, &key)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({ "key": key, "value": value })))
}

/// PUT /config/:key
#[utoipa::path(
    put, operation_id = "config_api_set_config",
    path = "/config/{key}",
    tag = "config",
    params(("key" = String, Path, description = "Config key")),
    request_body = Object,
    responses((status = 200, description = "Config value stored", body = Object))
)]
pub async fn set_config(
    State(state): State<AppState>,
    ValidatedPath(key): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::config::set_config(&*state.runtime.store, &key, body)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(StatusCode::OK)
}

/// POST /config/ai — set the colony AI adapter and hot-swap.
#[utoipa::path(
    post, operation_id = "config_api_set_ai_adapter",
    path = "/config/ai",
    tag = "config",
    request_body = Object,
    responses((status = 200, description = "AI adapter selected", body = Object))
)]
pub async fn set_ai_adapter(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::config::configure_ai_adapter(
        &state.runtime,
        operations::config::AiTarget::Colony,
        body,
    )
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "status": "swapped" })))
}

/// POST /config/connector/:name
#[utoipa::path(
    post, operation_id = "config_api_set_connector_config",
    path = "/config/connector/{name}",
    tag = "config",
    params(("name" = String, Path, description = "Connector name")),
    request_body = Object,
    responses((status = 200, description = "Connector config stored", body = Object))
)]
pub async fn set_connector_config(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::config::set_connector_config(&state.runtime, &name, body)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "status": "stored" })))
}

/// Body of `POST /config/ai/configure`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ConfigureAiBody {
    pub target: operations::config::AiTarget,
    pub config: serde_json::Value,
}

/// POST /config/ai/configure — configure AI at one level of the hierarchy.
#[utoipa::path(
    post, operation_id = "config_api_configure_ai_adapter",
    path = "/config/ai/configure",
    tag = "config",
    request_body = ConfigureAiBody,
    responses((status = 200, description = "AI adapter configured", body = Object))
)]
pub async fn configure_ai_adapter(
    State(state): State<AppState>,
    Json(body): Json<ConfigureAiBody>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::config::configure_ai_adapter(&state.runtime, body.target, body.config)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "status": "configured" })))
}

/// POST /connectors/{name}/upsert-config — setup if new, update if exists.
#[utoipa::path(
    post, operation_id = "config_api_upsert_connector_config",
    path = "/connectors/{name}/upsert-config",
    tag = "config",
    params(("name" = String, Path, description = "Connector name")),
    request_body = Object,
    responses((status = 200, description = "Connector config merged", body = Object))
)]
pub async fn upsert_connector_config(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let is_new = operations::config::upsert_connector_config(&state.runtime, &name, body)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "is_new": is_new })))
}

/// POST /formations/{id}/toggle-guard — toggle guard mode.
#[utoipa::path(
    post, operation_id = "config_api_toggle_formation_guard",
    path = "/formations/{id}/toggle-guard",
    tag = "config",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Guard toggled", body = Object))
)]
pub async fn toggle_formation_guard(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let enabled = operations::config::toggle_formation_guard(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "enabled": enabled })))
}
