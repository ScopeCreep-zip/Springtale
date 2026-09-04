//! Connector setup — configure and load a connector from its factory.

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Setup and load an available connector with config.
///
/// Finds the connector's factory by name, creates an instance with the
/// provided config, installs it in the registry, and persists the config
/// for next boot. This is the "Configure & Load" action from the UI.
pub async fn setup_connector(
    state: &RuntimeState,
    name: &str,
    config: serde_json::Value,
) -> Result<String, OperationError> {
    use springtale_connector::factory::FactoryEntry;

    // Find the factory for this connector
    let factory = inventory::iter::<FactoryEntry>
        .into_iter()
        .find(|e| e.factory.name() == name || e.factory.config_key() == name)
        .map(|e| e.factory)
        .ok_or_else(|| {
            OperationError::Validation(format!("no factory found for connector '{name}'"))
        })?;

    // Create the connector instance
    let connector = factory
        .create(config.clone())
        .await
        .map_err(|e| OperationError::Connector(format!("failed to create {name}: {e}")))?;

    // Install in registry. A name is registered once, so a re-configure
    // of an already-loaded connector removes the old entry first, under
    // the same write lock (mirrors `reload_connector`). A first-time
    // configure has nothing to remove.
    let registered_name = {
        let mut registry = state.registry.write().await;
        if registry.get(factory.name()).is_some() {
            registry
                .remove(factory.name())
                .map_err(|e| OperationError::Connector(format!("failed to remove {name}: {e}")))?;
        }
        registry
            .install_native(connector)
            .map_err(|e| OperationError::Connector(format!("failed to install {name}: {e}")))?
    };

    // Persist config for next boot — key uses the incoming name, matching
    // get_connector_config() and remove_connector() which also use {name}.
    let key = format!("connector:{name}");
    super::super::config::set_config(&*state.store, &key, config).await?;

    // Clear any prior removal flag (user is re-adding a previously removed connector)
    let removed_key = format!("connector-removed:{name}");
    let _ = state.store.delete_config(&removed_key).await;

    tracing::info!(connector = %registered_name, "connector configured and loaded");
    Ok(registered_name)
}
