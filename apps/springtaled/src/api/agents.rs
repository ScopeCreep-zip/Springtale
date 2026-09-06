use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /agents/states
#[utoipa::path(
    get, operation_id = "agents_list_states",
    path = "/agents/states",
    tag = "agents",
    responses((status = 200, description = "Aggregated agent states", body = Object))
)]
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
#[utoipa::path(
    get, operation_id = "agents_get_autonomy",
    path = "/agents/{name}/autonomy",
    tag = "agents",
    params(("name" = String, Path, description = "Agent name")),
    responses((status = 200, description = "Current autonomy level", body = Object))
)]
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
#[utoipa::path(
    put, operation_id = "agents_set_autonomy",
    path = "/agents/{name}/autonomy",
    tag = "agents",
    params(("name" = String, Path, description = "Agent name")),
    request_body = operations::agent::SetAutonomyRequest,
    responses((status = 200, description = "Updated autonomy level", body = Object))
)]
pub async fn set_autonomy(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(req): Json<operations::agent::SetAutonomyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let level = req.level.as_str();
    let target = agent_target(&state, &name).await?;
    operations::agent::set_autonomy(&*state.runtime.store, &target, level)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "name": name, "level": level })))
}

/// POST /agents/:name/autonomy/step — step autonomy up or down.
#[utoipa::path(
    post, operation_id = "agents_step_autonomy",
    path = "/agents/{name}/autonomy/step",
    tag = "agents",
    params(("name" = String, Path, description = "Agent name")),
    request_body = operations::agent::StepAutonomyRequest,
    responses((status = 200, description = "Stepped autonomy level", body = Object))
)]
pub async fn step_autonomy(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
    Json(req): Json<operations::agent::StepAutonomyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let target = agent_target(&state, &name).await?;
    let level = operations::agent::step_autonomy(&*state.runtime.store, &target, req.direction)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "name": name, "level": level })))
}
