use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /config/heartbeat
pub async fn get_heartbeat(State(state): State<AppState>) -> impl IntoResponse {
    let monitor = state.heartbeat_monitor.lock().await;
    Json(serde_json::json!({
        "interval_secs": monitor.interval_secs(),
        "enabled": monitor.is_running(),
    }))
}

/// PUT /config/heartbeat
pub async fn set_heartbeat(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let interval_secs = body
        .get("interval_secs")
        .and_then(|v| v.as_u64())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut monitor = state.heartbeat_monitor.lock().await;
    if interval_secs == 0 {
        monitor.stop();
    } else {
        monitor.set_interval(interval_secs);
    }

    Ok(Json(serde_json::json!({
        "interval_secs": monitor.interval_secs(),
        "enabled": monitor.is_running(),
    })))
}

/// GET /config — list all config entries.
pub async fn list_config(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let entries = operations::config::list_config(&*state.runtime.store)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "config": entries })))
}

/// GET /config/:key
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

/// POST /config/ai — set AI adapter and hot-swap.
pub async fn set_ai_adapter(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    operations::config::set_ai_adapter(&state.runtime, body)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "status": "swapped" })))
}

/// POST /config/connector/:name
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

/// POST /config/ai/configure — configure AI adapter with target key.
pub async fn configure_ai_adapter(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let target = body["target"].as_str().unwrap_or("ai:global");
    let config = body.get("config").cloned().unwrap_or(serde_json::Value::Null);
    operations::config::configure_ai_adapter(&state.runtime, target, config)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "status": "configured" })))
}

/// POST /connectors/{name}/upsert-config — setup if new, update if exists.
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
pub async fn toggle_formation_guard(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let enabled = operations::config::toggle_formation_guard(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "enabled": enabled })))
}
