use tauri::State;

use springtale_runtime::operations::connectors::{ConnectorInfo, ConnectorSchemaInfo};

use crate::state::AppState;

/// List all installed connectors.
#[tauri::command]
pub async fn list_connectors(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectorInfo>, String> {
    Ok(springtale_runtime::operations::connectors::list_connectors(&state.runtime).await)
}

/// List all available connectors (loaded + unloaded).
#[tauri::command]
pub async fn list_available_connectors(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::connectors::AvailableConnectorInfo>, String> {
    Ok(springtale_runtime::operations::connectors::list_available_connectors(&state.runtime).await)
}

/// Setup and load an available connector with config.
#[tauri::command]
pub async fn setup_connector(
    state: State<'_, AppState>,
    name: String,
    config: serde_json::Value,
) -> Result<String, String> {
    springtale_runtime::operations::connectors::setup_connector(&state.runtime, &name, config)
        .await
        .map_err(|e| e.to_string())
}

/// Enable a connector by name.
#[tauri::command]
pub async fn enable_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    springtale_runtime::operations::connectors::enable_connector(&state.runtime, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Disable a connector by name.
#[tauri::command]
pub async fn disable_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    springtale_runtime::operations::connectors::disable_connector(&state.runtime, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a connector from the registry.
#[tauri::command]
pub async fn remove_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    springtale_runtime::operations::connectors::remove_connector(&state.runtime, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a connector and all dependent rules.
#[tauri::command]
pub async fn remove_connector_cascade(state: State<'_, AppState>, name: String) -> Result<Vec<String>, String> {
    springtale_runtime::operations::connectors::remove_connector_cascade(&state.runtime, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Get a connector's current config.
#[tauri::command]
pub async fn get_connector_config(state: State<'_, AppState>, name: String) -> Result<serde_json::Value, String> {
    springtale_runtime::operations::connectors::get_connector_config(&*state.runtime.store, &name)
        .await
        .map_err(|e| e.to_string())
}

/// List recent execution results for a connector.
#[tauri::command]
pub async fn list_connector_outputs(
    state: State<'_, AppState>,
    name: String,
    limit: Option<usize>,
) -> Result<Vec<springtale_store::ExecutionResultRow>, String> {
    springtale_runtime::operations::connectors::list_connector_outputs(
        &*state.runtime.store,
        &name,
        limit.unwrap_or(20),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Install a connector from a manifest.
#[tauri::command]
pub async fn install_connector(
    state: State<'_, AppState>,
    manifest: springtale_connector::ConnectorManifest,
) -> Result<String, String> {
    springtale_runtime::operations::connectors::install_connector(&state.runtime, manifest)
        .await
        .map_err(|e| e.to_string())
}

/// Get all connector schemas with trigger/action declarations.
#[tauri::command]
pub async fn get_connector_schemas(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectorSchemaInfo>, String> {
    Ok(springtale_runtime::operations::connectors::get_connector_schemas(&state.runtime).await)
}
