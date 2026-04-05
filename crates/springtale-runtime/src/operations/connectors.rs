//! Connector operations — list, enable, disable, remove, install, schemas.
//!
//! Runtime operations take `&RuntimeState` (need engine/registry).
//! Store operations take `&dyn StorageBackend` (CLI uses these).

use serde::Serialize;

use springtale_store::StorageBackend;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Connector info for listing.
#[derive(Debug, Serialize)]
pub struct ConnectorInfo {
    pub name: String,
    pub enabled: bool,
}

/// List all installed connectors.
pub async fn list_connectors(state: &RuntimeState) -> Vec<ConnectorInfo> {
    let registry = state.registry.read().await;
    registry
        .list()
        .into_iter()
        .map(|(name, enabled)| ConnectorInfo {
            name: name.to_owned(),
            enabled,
        })
        .collect()
}

/// Enable a connector by name.
pub async fn enable_connector(state: &RuntimeState, name: &str) -> Result<(), OperationError> {
    let mut registry = state.registry.write().await;
    registry
        .enable(name)
        .map_err(|e| OperationError::Connector(format!("failed to enable {name}: {e}")))
}

/// Disable a connector by name.
pub async fn disable_connector(state: &RuntimeState, name: &str) -> Result<(), OperationError> {
    let mut registry = state.registry.write().await;
    registry
        .disable(name)
        .map_err(|e| OperationError::Connector(format!("failed to disable {name}: {e}")))
}

/// Remove a connector from the registry.
pub async fn remove_connector(state: &RuntimeState, name: &str) -> Result<(), OperationError> {
    let mut registry = state.registry.write().await;
    registry
        .remove(name)
        .map_err(|e| OperationError::Connector(format!("failed to remove {name}: {e}")))
}

/// Install a connector manifest — validates and registers in the store.
///
/// The manifest is validated for structure (name, version, no wildcard hosts).
/// If a signature is present, it's logged but verification is deferred to
/// Phase 2 (requires author public key registry).
pub async fn install_connector(
    state: &RuntimeState,
    manifest: springtale_connector::ConnectorManifest,
) -> Result<String, OperationError> {
    // Validate manifest structure
    springtale_connector::manifest::verify::verify_manifest(&manifest)
        .map_err(|e| OperationError::Validation(format!("manifest invalid: {e}")))?;

    if manifest.signature.is_some() {
        tracing::info!(
            connector = %manifest.name,
            "manifest has signature — verification requires author key registry (Phase 2)"
        );
    }

    let manifest_json = serde_json::to_string(&manifest)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;

    let row = springtale_store::schema::connectors::ConnectorRow {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        description: manifest.description.clone(),
        manifest_json,
        enabled: true,
        installed_at: chrono::Utc::now(),
    };

    state.store.register_connector(&row).await?;

    let name = manifest.name;
    tracing::info!(connector = %name, "connector manifest registered");
    Ok(name)
}

// ── Schema introspection ─────────────────────────────────────────────────────

/// Connector schema info — shared between springtaled API and desktop IPC.
#[derive(Debug, Serialize, Clone)]
pub struct ConnectorSchemaInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub triggers: Vec<TriggerSchemaInfo>,
    pub actions: Vec<ActionSchemaInfo>,
}

/// Trigger declaration info for schema introspection.
#[derive(Debug, Serialize, Clone)]
pub struct TriggerSchemaInfo {
    pub name: String,
    pub description: String,
    pub schema: Option<serde_json::Value>,
}

/// Action declaration info for schema introspection.
#[derive(Debug, Serialize, Clone)]
pub struct ActionSchemaInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

/// Get all connector schemas with trigger/action declarations.
///
/// Reads registry manifests — read-only, no store needed.
pub async fn get_connector_schemas(state: &RuntimeState) -> Vec<ConnectorSchemaInfo> {
    let registry = state.registry.read().await;
    registry
        .list()
        .into_iter()
        .filter_map(|(name, _)| {
            let entry = registry.get(name)?;
            let manifest = entry.host.manifest();
            Some(ConnectorSchemaInfo {
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                description: manifest.description.clone(),
                triggers: entry
                    .host
                    .triggers()
                    .iter()
                    .map(|t| TriggerSchemaInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        schema: t.schema.clone(),
                    })
                    .collect(),
                actions: entry
                    .host
                    .actions()
                    .iter()
                    .map(|a| ActionSchemaInfo {
                        name: a.name.clone(),
                        description: a.description.clone(),
                        input_schema: a.input_schema.clone(),
                        output_schema: a.output_schema.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
}

// ── Store-only operations (CLI) ──────────────────────────────────────────────

/// List connectors from the persistent store (no registry needed).
///
/// Used by CLI which doesn't load the full runtime.
pub async fn list_connectors_from_store(
    store: &dyn StorageBackend,
) -> Result<Vec<springtale_store::schema::connectors::ConnectorRow>, OperationError> {
    store.list_connectors().await.map_err(OperationError::Store)
}

/// Enable a connector in the persistent store.
pub async fn enable_connector_in_store(
    store: &dyn StorageBackend,
    name: &str,
) -> Result<(), OperationError> {
    store
        .set_connector_enabled(name, true)
        .await
        .map_err(OperationError::Store)
}

/// Disable a connector in the persistent store.
pub async fn disable_connector_in_store(
    store: &dyn StorageBackend,
    name: &str,
) -> Result<(), OperationError> {
    store
        .set_connector_enabled(name, false)
        .await
        .map_err(OperationError::Store)
}

/// Remove a connector from the persistent store.
pub async fn remove_connector_from_store(
    store: &dyn StorageBackend,
    name: &str,
) -> Result<(), OperationError> {
    store
        .remove_connector(name)
        .await
        .map_err(OperationError::Store)
}

/// Install a connector manifest to the persistent store — validates and persists.
///
/// Store-only variant: does not load into the in-memory registry.
/// Used by CLI, which writes to the DB for springtaled to pick up on next start.
pub async fn install_connector_to_store(
    store: &dyn StorageBackend,
    manifest: springtale_connector::ConnectorManifest,
) -> Result<String, OperationError> {
    springtale_connector::manifest::verify::verify_manifest(&manifest)
        .map_err(|e| OperationError::Validation(format!("manifest invalid: {e}")))?;

    if manifest.signature.is_some() {
        tracing::info!(
            connector = %manifest.name,
            "manifest has signature — verification requires author key registry (Phase 2)"
        );
    }

    let manifest_json = serde_json::to_string(&manifest)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;

    let row = springtale_store::schema::connectors::ConnectorRow {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        description: manifest.description.clone(),
        manifest_json,
        enabled: true,
        installed_at: chrono::Utc::now(),
    };

    store.register_connector(&row).await?;

    let name = manifest.name;
    tracing::info!(connector = %name, "connector manifest registered (store-only)");
    Ok(name)
}
