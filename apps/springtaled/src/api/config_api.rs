use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::state::AppState;

/// GET /config/heartbeat — get current heartbeat configuration.
pub async fn get_heartbeat(State(state): State<AppState>) -> impl IntoResponse {
    let monitor = state.heartbeat_monitor.lock().await;
    Json(serde_json::json!({
        "interval_secs": monitor.interval_secs(),
        "enabled": monitor.is_running(),
    }))
}

/// PUT /config/heartbeat — update heartbeat interval.
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
        tracing::info!("heartbeat disabled");
    } else {
        monitor.set_interval(interval_secs);
        tracing::info!(interval_secs, "heartbeat interval updated");
    }

    Ok(Json(serde_json::json!({
        "interval_secs": monitor.interval_secs(),
        "enabled": monitor.is_running(),
    })))
}
