use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /formations — list all formations.
#[utoipa::path(
    get, operation_id = "formations_list",
    path = "/formations",
    tag = "formations",
    responses((status = 200, description = "All formations", body = Vec<Object>))
)]
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
#[utoipa::path(
    get, operation_id = "formations_get",
    path = "/formations/{id}",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "One formation", body = Object))
)]
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
#[utoipa::path(
    get, operation_id = "formations_commands",
    path = "/formations/{id}/commands",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Command grid for the formation", body = Vec<Object>))
)]
pub async fn commands(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    match operations::commands::formation_available_commands(&state.runtime, &id).await {
        Ok(cmds) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "commands": cmds })),
        )),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /formations/{id}/run-command — generic command dispatcher. Body:
/// `{ "command_id": "formation:rally", "params": {…}? }`. ALL command→action
/// mapping lives in the backend so the frontend just forwards the clicked id.
#[utoipa::path(
    post, operation_id = "formations_run_command",
    path = "/formations/{id}/run-command",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    request_body = Object,
    responses((status = 200, description = "Command outcome", body = Object))
)]
pub async fn run_command(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let Some(command_id) = body.get("command_id").and_then(|v| v.as_str()) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let params = body.get("params");
    match operations::commands::run_formation_command(&state.runtime, &id, command_id, params).await
    {
        Ok(()) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "ran": command_id })),
        )),
        Err(e) => {
            tracing::warn!(command = command_id, error = %e, "run_command failed");
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// GET /formations/{id}/members/eligible — eligible-removal list for the
/// RM MBR overlay. Backend decides which members are removable; frontend
/// renders the list.
#[utoipa::path(
    get, operation_id = "formations_eligible_members",
    path = "/formations/{id}/members/eligible",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Agents eligible to join", body = Vec<Object>))
)]
pub async fn eligible_members(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    match operations::commands::formation_eligible_members(&state.runtime, &id).await {
        Ok(members) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "members": members })),
        )),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /formations — create a new formation.
#[utoipa::path(
    post, operation_id = "formations_create",
    path = "/formations",
    tag = "formations",
    request_body = operations::formations::CreateFormationRequest,
    responses((status = 200, description = "Formation created", body = Object))
)]
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<operations::formations::CreateFormationRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let id = operations::formations::create_formation(
        &state.runtime,
        req.name,
        req.intent,
        req.connectors,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to create formation");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// POST /formations/{id}/deploy — deploy a formation.
#[utoipa::path(
    post, operation_id = "formations_deploy",
    path = "/formations/{id}/deploy",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Formation deployed", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "formations_pause",
    path = "/formations/{id}/pause",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Formation paused", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "formations_resume",
    path = "/formations/{id}/resume",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Formation resumed", body = Object))
)]
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
#[utoipa::path(
    put, operation_id = "formations_update_intent",
    path = "/formations/{id}/intent",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    request_body = Object,
    responses((status = 200, description = "Intent updated", body = Object))
)]
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

/// POST /formations/{id}/propose-intent — open a consensus vote to change
/// the formation's intent (§5.5 formation self-governance, Fever-gated).
#[utoipa::path(
    post, operation_id = "formations_propose_intent",
    path = "/formations/{id}/propose-intent",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    request_body = Object,
    responses((status = 200, description = "Intent proposal opened", body = Object))
)]
pub async fn propose_intent(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let intent = body["intent"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    operations::formations::propose_intent_change(&state.runtime, &id, intent)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "proposed": id }))))
}

/// POST /formations/{id}/votes/{vote_id} — cast a ballot on an open
/// consensus vote (§11). Body: { "voter": "<agent uuid>", "approve": bool }.
#[utoipa::path(
    post, operation_id = "formations_cast_vote",
    path = "/formations/{id}/votes/{vote_id}",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id"), ("vote_id" = String, Path, description = "Vote id")),
    request_body = Object,
    responses((status = 200, description = "Vote recorded", body = Object))
)]
pub async fn cast_vote(
    State(state): State<AppState>,
    axum::extract::Path((id, vote_id)): axum::extract::Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let voter = body["voter"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let approve = body["approve"].as_bool().ok_or(StatusCode::BAD_REQUEST)?;
    operations::formations::cast_vote(&state.runtime, &id, &vote_id, voter, approve)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "voted": vote_id })),
    ))
}

/// POST /formations/{id}/members — add a member to a formation.
#[utoipa::path(
    post, operation_id = "formations_add_member",
    path = "/formations/{id}/members",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    request_body = Object,
    responses((status = 200, description = "Member added", body = Object))
)]
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
#[utoipa::path(
    delete, operation_id = "formations_remove_member",
    path = "/formations/{id}/members",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    request_body = Object,
    responses((status = 200, description = "Member removed", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "formations_dissolve",
    path = "/formations/{id}/dissolve",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Formation dissolved", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "formations_rally",
    path = "/formations/{id}/rally",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Formation rallied", body = Object))
)]
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
#[utoipa::path(
    get, operation_id = "formations_list_intents",
    path = "/formations/intents",
    tag = "formations",
    responses((status = 200, description = "Selectable intents", body = Vec<Object>))
)]
pub async fn list_intents() -> impl IntoResponse {
    let intents = operations::formations::list_intents();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "intents": intents })),
    )
}

/// POST /formations/deploy-team — deploy a complete team atomically.
#[utoipa::path(
    post, operation_id = "formations_deploy_team",
    path = "/formations/deploy-team",
    tag = "formations",
    request_body = operations::formations::TeamDeployRequest,
    responses((status = 200, description = "Team deployed", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "formations_cycle_intent",
    path = "/formations/{id}/cycle-intent",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Intent cycled", body = Object))
)]
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
#[utoipa::path(
    post, operation_id = "formations_cycle_autonomy",
    path = "/formations/{id}/cycle-autonomy",
    tag = "formations",
    params(("id" = String, Path, description = "Formation id")),
    responses((status = 200, description = "Autonomy cycled", body = Object))
)]
pub async fn cycle_autonomy(
    State(state): State<AppState>,
    ValidatedPath(id): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let level = operations::formations::cycle_autonomy(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "level": level }))))
}
