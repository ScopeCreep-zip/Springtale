//! Store-only connector operations (CLI).
//!
//! These functions operate on the persistent store without needing
//! the full runtime (no registry, no engine). The CLI uses these
//! to manage connectors while springtaled picks up changes on next start.

use springtale_store::StorageBackend;

use crate::error::OperationError;

use super::install::verify_manifest_sig_if_present;

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

    // Verify Ed25519 signature if present using trusted author registry
    verify_manifest_sig_if_present(&manifest, store).await?;

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
