use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// GET /formations — list all formations.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    match operations::formations::list_formations(&state.runtime).await {
        Ok(formations) => (StatusCode::OK, Json(serde_json::json!({ "formations": formations }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
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

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id })),
    ))
}

/// POST /formations/{id}/deploy — deploy a formation.
pub async fn deploy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    operations::formations::deploy_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "deployed": id }))))
}

/// POST /formations/{id}/pause — pause a formation.
pub async fn pause(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    operations::formations::pause_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "paused": id }))))
}

/// POST /formations/{id}/resume — resume a formation.
pub async fn resume(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    operations::formations::resume_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "resumed": id }))))
}

/// POST /formations/{id}/dissolve — dissolve a formation.
pub async fn dissolve(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&id)?;
    operations::formations::dissolve_formation(&state.runtime, &id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "dissolved": id }))))
}
