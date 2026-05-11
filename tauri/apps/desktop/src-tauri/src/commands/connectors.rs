use tauri::State;

use springtale_runtime::operations::connectors::{ConnectorInfo, ConnectorSchemaInfo};

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

#[tauri::command]
pub async fn list_connectors(state: State<'_, AppState>) -> Result<Vec<ConnectorInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::connectors::list_connectors(rt).await)
}

#[tauri::command]
pub async fn list_available_connectors(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::connectors::AvailableConnectorInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::connectors::list_available_connectors(rt).await)
}

#[tauri::command]
pub async fn setup_connector(
    state: State<'_, AppState>,
    name: String,
    config: serde_json::Value,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::setup_connector(rt, &name, config)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn enable_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::enable_connector(rt, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn disable_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::disable_connector(rt, &name)
        .await
        .map_err(|e| e.to_string())
}

/// G4 — hot-reload a connector mid-mission. Thin IPC pass-through; the
/// runtime op handles the atomic swap and in-flight call preservation.
#[tauri::command]
pub async fn reload_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::reload_connector(rt, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_connector(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::remove_connector(rt, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_connector_cascade(
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<String>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::remove_connector_cascade(rt, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_connector_config(
    state: State<'_, AppState>,
    name: String,
) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::get_connector_config(&*rt.store, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_connector_outputs(
    state: State<'_, AppState>,
    name: String,
    limit: Option<usize>,
) -> Result<Vec<springtale_store::ExecutionResultRow>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::list_connector_outputs(
        &*rt.store,
        &name,
        limit.unwrap_or(20),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_connector(
    state: State<'_, AppState>,
    manifest: springtale_connector::ConnectorManifest,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::connectors::install_connector(rt, manifest)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_connector_schemas(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectorSchemaInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    Ok(springtale_runtime::operations::connectors::get_connector_schemas(rt).await)
}
