use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::state::AppState;

/// GET /health — liveness probe.
///
/// Returns 200 OK if the process is running. No authentication required.
#[utoipa::path(
    get, operation_id = "health_health",
    path = "/health",
    tag = "health",
    security(()),
    responses((status = 200, description = "Liveness probe", body = Object))
)]
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

/// GET /ready — readiness probe.
///
/// Returns 200 OK if the daemon has completed its boot sequence and
/// all subsystems are accessible. Returns 503 if not ready.
/// No authentication required.
#[utoipa::path(
    get, operation_id = "health_ready",
    path = "/ready",
    tag = "health",
    security(()),
    responses((status = 200, description = "Readiness probe", body = Object))
)]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    if !state.is_ready() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "booting" })),
        );
    }

    // Verify store is accessible by listing rules (lightweight query)
    let store_ok = state.runtime.store.list_rules().await.is_ok();

    if store_ok {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ready",
                "store": "ok",
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "degraded",
                "store": "error",
            })),
        )
    }
}
