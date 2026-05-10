use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /formations — list all formations.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    match operations::formations::list_formations(&state.runtime).await {
        Ok(formations) => (
            StatusCode::OK,
            Json(serde_json::json!({ "formations": formations })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /formations/{id} — get a single formation.
pub async fn get(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    match operations::formations::get_formation(&state.runtime, &id).await {
        Ok(formation) => Ok((StatusCode::OK, Json(serde_json::json!(formation)))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// GET /formations/{id}/commands — backend-supplied 3×3 command grid with
/// status-aware enable/disable. Frontend renders the list; no eligibility
/// logic on the JS side.
pub async fn commands(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    match operations::commands::formation_available_commands(&state.runtime, &id).await {
        Ok(cmds) => Ok((StatusCode::OK, Json(serde_json::json!({ "commands": cmds })))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// GET /formations/{id}/members/eligible — eligible-removal list for the
/// RM MBR overlay. Backend decides which members are removable; frontend
/// renders the list.
pub async fn eligible_members(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    match operations::commands::formation_eligible_members(&state.runtime, &id).await {
        Ok(members) => Ok((StatusCode::OK, Json(serde_json::json!({ "members": members })))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /formations — create a new formation.
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let intent = body["intent"].as_str().unwrap_or("Reconnoiter");
    let connectors: Vec<String> = body["connectors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default();

    let id = operations::formations::create_formation(
        &state.runtime,
        name.to_owned(),
        intent.to_owned(),
        connectors,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to create formation");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// POST /formations/{id}/deploy — deploy a formation.
pub async fn deploy(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::formations::deploy_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "deployed": id }))))
}

/// POST /formations/{id}/pause — pause a formation.
pub async fn pause(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::formations::pause_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "paused": id }))))
}

/// POST /formations/{id}/resume — resume a formation.
pub async fn resume(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::formations::resume_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "resumed": id }))))
}

/// PUT /formations/{id}/intent — update formation intent.
pub async fn update_intent(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let intent = body["intent"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    operations::formations::update_intent(&state.runtime, &id, intent)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "updated": id }))))
}

/// POST /formations/{id}/members — add a member to a formation.
pub async fn add_member(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let connector_name = body["connector_name"]
        .as_str()
        .ok_or(StatusCode::BAD_REQUEST)?;
    operations::formations::add_member(&state.runtime, &id, connector_name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "added": connector_name })),
    ))
}

/// DELETE /formations/{id}/members — remove a member from a formation.
pub async fn remove_member(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let connector_name = body["connector_name"]
        .as_str()
        .ok_or(StatusCode::BAD_REQUEST)?;
    operations::formations::remove_member(&state.runtime, &id, connector_name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "removed": connector_name })),
    ))
}

/// POST /formations/{id}/dissolve — dissolve a formation.
pub async fn dissolve(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::formations::dissolve_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "dissolved": id }))))
}

/// POST /formations/{id}/rally — manually trigger self-rally.
pub async fn rally(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::formations::rally_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "rallied": id }))))
}

/// GET /formations/intents — list valid formation intents.
pub async fn list_intents() -> impl IntoResponse {
    let intents = operations::formations::list_intents();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "intents": intents })),
    )
}

/// POST /formations/deploy-team — deploy a complete team atomically.
pub async fn deploy_team(
    State(state): State<AppState>,
    Json(team): Json<operations::formations::TeamDeployRequest>,
) -> impl IntoResponse {
    match operations::formations::deploy_team(&state.runtime, team).await {
        Ok(result) => (StatusCode::CREATED, Json(serde_json::json!(result))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /formations/{id}/cycle-intent — cycle formation intent.
pub async fn cycle_intent(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let intent = operations::formations::cycle_intent(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "intent": intent })),
    ))
}

/// POST /formations/{id}/cycle-autonomy — cycle formation autonomy.
pub async fn cycle_autonomy(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let level = operations::formations::cycle_autonomy(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "level": level }))))
}
