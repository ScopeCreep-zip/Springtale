use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /agents/states
pub async fn list_states(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let states = operations::agent::list_agent_states(&state.runtime)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "agents": states })))
}

/// Resolve the `:name` path segment (a rule name or rule id) to the
/// rule-id-keyed autonomy target. Unknown names are 404.
async fn agent_target(
    state: &AppState,
    name_or_id: &str,
) -> Result<operations::agent::AutonomyTarget, StatusCode> {
    operations::agent::resolve_agent_target(&state.runtime, name_or_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)
}

/// GET /agents/:name/autonomy — `:name` is a rule name or rule id.
pub async fn get_autonomy(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let target = agent_target(&state, &name).await?;
    let level = operations::agent::get_autonomy(&*state.runtime.store, &target)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({ "name": name, "level": level.as_str() }),
    ))
}

/// PUT /agents/:name/autonomy — `:name` is a rule name or rule id.
pub async fn set_autonomy(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let level = body
        .get("level")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let target = agent_target(&state, &name).await?;
    operations::agent::set_autonomy(&*state.runtime.store, &target, level)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "name": name, "level": level })))
}

/// POST /agents/:name/autonomy/step — step autonomy up or down.
pub async fn step_autonomy(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let direction_str = body
        .get("direction")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let direction: operations::agent::AutonomyDirection =
        serde_json::from_value(serde_json::Value::String(direction_str.to_owned()))
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    let target = agent_target(&state, &name).await?;
    let level = operations::agent::step_autonomy(&*state.runtime.store, &target, direction)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "name": name, "level": level })))
}
