use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_runtime::operations;

use super::extractors::ValidatedPath;
use super::state::AppState;

/// GET /connectors — list all installed connectors.
#[utoipa::path(
    get, operation_id = "connectors_list",
    path = "/connectors",
    tag = "connectors",
    responses((status = 200, description = "Installed connectors", body = Vec<Object>))
)]
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let connectors = operations::connectors::list_connectors(&state.runtime).await;
    Json(serde_json::json!({ "connectors": connectors }))
}

/// GET /connectors/available — list ALL connectors from factory registry.
#[utoipa::path(
    get, operation_id = "connectors_list_available",
    path = "/connectors/available",
    tag = "connectors",
    responses((status = 200, description = "Connectors available to install", body = Vec<Object>))
)]
pub async fn list_available(State(state): State<AppState>) -> impl IntoResponse {
    let available = operations::connectors::list_available_connectors(&state.runtime).await;
    Json(serde_json::json!({ "available": available }))
}

/// POST /connectors/setup — configure and load an available connector.
#[utoipa::path(
    post, operation_id = "connectors_setup",
    path = "/connectors/setup",
    tag = "connectors",
    request_body = Object,
    responses((status = 200, description = "Connector set up", body = Object))
)]
pub async fn setup(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let config = body.get("config").cloned().unwrap_or(serde_json::json!({}));
    let registered = operations::connectors::setup_connector(&state.runtime, name, config)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "name": registered })),
    ))
}

/// DELETE /connectors/{name} — remove a connector.
#[utoipa::path(
    delete, operation_id = "connectors_remove",
    path = "/connectors/{name}",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector removed", body = Object))
)]
pub async fn remove(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::connectors::remove_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "removed": name }))))
}

/// DELETE /connectors/{name}/cascade — remove connector + dependent rules.
#[utoipa::path(
    delete, operation_id = "connectors_remove_cascade",
    path = "/connectors/{name}/cascade",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector and dependent rules removed", body = Object))
)]
pub async fn remove_cascade(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let deleted = operations::connectors::remove_connector_cascade(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "removed": name, "rules_deleted": deleted })),
    ))
}

/// GET /connectors/{name}/config — get connector config.
#[utoipa::path(
    get, operation_id = "connectors_get_config",
    path = "/connectors/{name}/config",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector config (secrets redacted)", body = Object))
)]
pub async fn get_config(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let config = operations::connectors::get_connector_config(&*state.runtime.store, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(
        serde_json::json!({ "connector": name, "config": config }),
    ))
}

/// GET /connectors/{name}/outputs — list recent execution results.
#[utoipa::path(
    get, operation_id = "connectors_list_outputs",
    path = "/connectors/{name}/outputs",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Declared connector outputs", body = Vec<Object>))
)]
pub async fn list_outputs(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    let outputs = operations::connectors::list_connector_outputs(&*state.runtime.store, &name, 20)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "outputs": outputs })))
}

/// POST /connectors/{name}/enable — enable a connector.
#[utoipa::path(
    post, operation_id = "connectors_enable",
    path = "/connectors/{name}/enable",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector enabled", body = Object))
)]
pub async fn enable(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::connectors::enable_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "enabled": name }))))
}

/// POST /connectors/{name}/disable — disable a connector.
#[utoipa::path(
    post, operation_id = "connectors_disable",
    path = "/connectors/{name}/disable",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector disabled", body = Object))
)]
pub async fn disable(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::connectors::disable_connector(&state.runtime, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "disabled": name })),
    ))
}

/// POST /connectors/{name}/reload — G4 hot-reload a connector without
/// dropping in-flight calls. The connector's persisted config is
/// re-applied verbatim; subsequent dispatches land on the new host.
#[utoipa::path(
    post, operation_id = "connectors_reload",
    path = "/connectors/{name}/reload",
    tag = "connectors",
    params(("name" = String, Path, description = "Connector name")),
    responses((status = 200, description = "Connector reloaded", body = Object))
)]
pub async fn reload(
    State(state): State<AppState>,
    ValidatedPath(name): ValidatedPath,
) -> Result<impl IntoResponse, StatusCode> {
    operations::connectors::reload_connector(&state.runtime, &name)
        .await
        .map_err(|e| {
            tracing::warn!(connector = %name, error = %e, "reload failed");
            StatusCode::BAD_REQUEST
        })?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "reloaded": name })),
    ))
}

/// GET /connectors/schemas — return all connector manifests with trigger/action schemas.
#[utoipa::path(
    get, operation_id = "connectors_schemas",
    path = "/connectors/schemas",
    tag = "connectors",
    responses((status = 200, description = "Config JSON schemas per connector", body = Object))
)]
pub async fn schemas(State(state): State<AppState>) -> impl IntoResponse {
    let schemas = operations::connectors::get_connector_schemas(&state.runtime).await;
    Json(serde_json::json!({ "manifests": schemas }))
}

/// POST /connectors/install — install a connector from manifest JSON.
#[utoipa::path(
    post, operation_id = "connectors_install",
    path = "/connectors/install",
    tag = "connectors",
    request_body = Object,
    responses((status = 200, description = "Connector installed", body = Object))
)]
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

/// POST /connectors/install-wasm — multipart with a `manifest` part
/// (JSON text) and a `wasm` part (binary). Signature, hash and
/// capability checks all live in the operation.
#[utoipa::path(
    post, operation_id = "connectors_install_wasm",
    path = "/connectors/install-wasm",
    tag = "connectors",
    request_body = Vec<u8>,
    responses((status = 200, description = "WASM connector installed", body = Object))
)]
pub async fn install_wasm(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut manifest: Option<springtale_connector::ConnectorManifest> = None;
    let mut wasm: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().map(str::to_owned);
        match name.as_deref() {
            Some("manifest") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                manifest = Some(
                    serde_json::from_str(&text)
                        .map_err(|e| (StatusCode::BAD_REQUEST, format!("manifest: {e}")))?,
                );
            }
            Some("wasm") => {
                wasm = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }
    let manifest = manifest.ok_or((
        StatusCode::BAD_REQUEST,
        "missing multipart part 'manifest'".to_owned(),
    ))?;
    let wasm = wasm.ok_or((
        StatusCode::BAD_REQUEST,
        "missing multipart part 'wasm'".to_owned(),
    ))?;
    let name = operations::connectors::install_wasm_connector(&state.runtime, wasm, manifest)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "wasm connector install failed");
            (StatusCode::BAD_REQUEST, e.to_string())
        })?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "installed": name })),
    ))
}
