use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::state::AppState;

/// GET /connectors — list all installed connectors.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let connectors = operations::connectors::list_connectors(&state.runtime).await;
    Json(serde_json::json!({ "connectors": connectors }))
}

/// DELETE /connectors/{name} — remove a connector.
pub async fn remove(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&name)?;
    operations::connectors::remove_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "removed": name }))))
}

/// POST /connectors/{name}/enable — enable a connector.
pub async fn enable(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&name)?;
    operations::connectors::enable_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "enabled": name }))))
}

/// POST /connectors/{name}/disable — disable a connector.
pub async fn disable(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&name)?;
    operations::connectors::disable_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "disabled": name })),
    ))
}

/// GET /connectors/schemas — return all connector manifests with trigger/action schemas.
pub async fn schemas(State(state): State<AppState>) -> impl IntoResponse {
    let schemas = operations::connectors::get_connector_schemas(&state.runtime).await;
    Json(serde_json::json!({ "manifests": schemas }))
}

/// POST /connectors/install — install a connector from manifest JSON.
pub async fn install(
    State(state): State<AppState>,
    Json(manifest): Json<springtale_connector::ConnectorManifest>,
) -> Result<impl IntoResponse, StatusCode> {
    let name = operations::connectors::install_connector(&state.runtime, manifest)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "connector install failed");
            StatusCode::BAD_REQUEST
        })?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "installed": name })),
    ))
}
