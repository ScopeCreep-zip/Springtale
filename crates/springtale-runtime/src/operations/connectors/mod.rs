//! Connector operations — list, enable, disable, remove, install, schemas.
//!
//! Runtime operations take `&RuntimeState` (need engine/registry).
//! Store operations take `&dyn StorageBackend` (CLI uses these).

mod install;
mod schema;
mod setup;
mod store;

use serde::Serialize;

use crate::error::OperationError;
use crate::state::RuntimeState;

// Re-export everything public from submodules
pub use install::{install_connector, install_wasm_connector};
pub use schema::{
    ActionSchemaInfo, AvailableConnectorInfo, ConnectorSchemaInfo, TriggerSchemaInfo,
    get_connector_schemas, list_available_connectors,
};
pub use setup::setup_connector;
pub use store::{
    disable_connector_in_store, enable_connector_in_store, install_connector_to_store,
    list_connectors_from_store, remove_connector_from_store,
};

// ── Core operations ─────────────────────────────────────────────────────────

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

/// Remove a connector — from registry, store, and config.
///
/// Cleans up all persistent state and marks the connector as removed
/// so it doesn't auto-reload on restart (even if it doesn't require config).
pub async fn remove_connector(state: &RuntimeState, name: &str) -> Result<(), OperationError> {
    // Drop any community roles this connector contributed (§14.4 / Phase
    // 21) before removing the manifest — once the manifest is gone we
    // can't look up the role names.
    deregister_manifest_roles_for(state, name).await;

    // Remove from in-memory registry
    {
        let mut registry = state.registry.write().await;
        registry
            .remove(name)
            .map_err(|e| OperationError::Connector(format!("failed to remove {name}: {e}")))?;
    }
    // Remove from persistent store (same method CLI uses)
    let _ = state.store.remove_connector(name).await;
    // Remove dynamic config entry (written by setupConnector)
    let config_key = format!("connector:{name}");
    let _ = state.store.delete_config(&config_key).await;
    // Mark as explicitly removed so init_registry won't auto-load it
    let removed_key = format!("connector-removed:{name}");
    let _ = state.store.set_config(&removed_key, "true").await;
    Ok(())
}

/// Look up the connector manifest by name and unregister any community
/// roles it contributed. Silently no-ops if the manifest isn't found or
/// isn't parseable — removal should still proceed if the DB row is
/// corrupt. Uses `list_connectors` because the store trait currently
/// exposes no `get_connector`; the list is small (one row per installed
/// connector) so the linear scan is fine.
async fn deregister_manifest_roles_for(state: &RuntimeState, name: &str) {
    let Ok(rows) = state.store.list_connectors().await else {
        return;
    };
    let Some(row) = rows.into_iter().find(|r| r.name == name) else {
        return;
    };
    if let Ok(manifest) = serde_json::from_str::<springtale_connector::ConnectorManifest>(
        &row.manifest_json,
    ) {
        crate::cooperation::unregister_manifest_roles(&state.role_registry, &manifest);
    }
}

/// Remove a connector and all rules that depend on it.
///
/// Returns IDs of deleted rules so the UI can show what was cleaned up.
/// This is the compound operation the frontend should call instead of
/// manually finding and deleting rules one by one.
pub async fn remove_connector_cascade(
    state: &RuntimeState,
    name: &str,
) -> Result<Vec<String>, OperationError> {
    // Find rules whose trigger references this connector
    let all_rules = state
        .store
        .list_rules()
        .await
        .map_err(OperationError::Store)?;
    let mut deleted_ids = Vec::new();

    for rule in &all_rules {
        // Check if this rule's trigger mentions the connector
        let trigger_json = serde_json::to_string(&rule.trigger).unwrap_or_default();
        if trigger_json.contains(name) {
            let rule_id = rule.id;
            {
                let mut engine = state.engine.write().await;
                engine.remove_rule(&rule_id);
            }
            state.store.delete_rule(&rule_id).await?;
            deleted_ids.push(rule_id.0.to_string());
        }
    }

    // Drop community roles before the manifest is removed.
    deregister_manifest_roles_for(state, name).await;

    // Remove connector from registry
    {
        let mut registry = state.registry.write().await;
        let _ = registry.remove(name);
    }
    // Remove from persistent store (same method CLI uses)
    let _ = state.store.remove_connector(name).await;
    // Remove dynamic config entry (written by setupConnector)
    let config_key = format!("connector:{name}");
    let _ = state.store.delete_config(&config_key).await;
    // Mark as explicitly removed so init_registry won't auto-load it
    let removed_key = format!("connector-removed:{name}");
    let _ = state.store.set_config(&removed_key, "true").await;

    tracing::info!(
        connector = name,
        rules_deleted = deleted_ids.len(),
        "connector cascade removed"
    );
    Ok(deleted_ids)
}

/// List recent execution results (outputs) for a connector.
pub async fn list_connector_outputs(
    store: &dyn springtale_store::StorageBackend,
    name: &str,
    limit: usize,
) -> Result<Vec<springtale_store::ExecutionResultRow>, OperationError> {
    store
        .list_execution_results(name, limit)
        .await
        .map_err(OperationError::Store)
}

/// Get a connector's current configuration from the config store.
pub async fn get_connector_config(
    store: &dyn springtale_store::StorageBackend,
    name: &str,
) -> Result<serde_json::Value, OperationError> {
    let key = format!("connector:{name}");
    super::config::get_config(store, &key).await
}
