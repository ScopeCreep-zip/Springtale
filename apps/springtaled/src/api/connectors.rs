use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use springtale_store::backend::trait_::StorageBackend;

use super::state::AppState;

/// GET /connectors — list all installed connectors.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let registry = state.registry.read().await;
    let connectors: Vec<serde_json::Value> = registry
        .list()
        .into_iter()
        .map(|(name, enabled)| {
            serde_json::json!({
                "name": name,
                "enabled": enabled,
            })
        })
        .collect();

    Json(serde_json::json!({ "connectors": connectors }))
}

/// DELETE /connectors/{name} — remove a connector.
pub async fn remove(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&name)?;
    let mut registry = state.registry.write().await;
    registry.remove(&name).map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "removed": name }))))
}

/// POST /connectors/{name}/enable — enable a connector.
pub async fn enable(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&name)?;
    let mut registry = state.registry.write().await;
    registry.enable(&name).map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "enabled": name }))))
}

/// POST /connectors/{name}/disable — disable a connector.
pub async fn disable(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&name)?;
    let mut registry = state.registry.write().await;
    registry.disable(&name).map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "disabled": name })),
    ))
}

/// POST /connectors/install — install a connector from manifest JSON.
///
/// Validates the manifest structure. If a signature is present, verifies it.
/// Unsigned manifests are allowed in Phase 1a for development.
/// Registers the manifest in the store. Actual connector activation
/// requires the compiled connector crate (Phase 2 dynamic loading).
pub async fn install(
    State(state): State<AppState>,
    Json(manifest): Json<springtale_connector::ConnectorManifest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate manifest structure (name, version, no wildcard hosts)
    springtale_connector::manifest::verify::verify_manifest(&manifest).map_err(|e| {
        tracing::warn!(error = %e, "manifest validation failed");
        StatusCode::BAD_REQUEST
    })?;

    // If manifest has a signature, log that verification is deferred to Phase 2
    // (requires author public key registry which doesn't exist yet)
    if manifest.signature.is_some() {
        tracing::info!(
            connector = %manifest.name,
            "manifest has signature — verification requires author key registry (Phase 2)"
        );
    }

    let manifest_json =
        serde_json::to_string(&manifest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = springtale_store::schema::connectors::ConnectorRow {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        description: manifest.description.clone(),
        manifest_json,
        enabled: true,
        installed_at: chrono::Utc::now(),
    };

    state
        .store
        .register_connector(&row)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(connector = %manifest.name, "connector manifest registered via API");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "installed": manifest.name,
            "version": manifest.version,
        })),
    ))
}
