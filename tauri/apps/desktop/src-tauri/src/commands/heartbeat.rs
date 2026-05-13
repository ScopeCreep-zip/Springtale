use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Get the heartbeat interval config value.
#[tauri::command]
#[specta::specta]
pub async fn get_heartbeat(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::config::get_config(&*rt.store, "heartbeat_interval")
        .await
        .map_err(|e| e.to_string())
}

/// Set the heartbeat interval config value.
#[tauri::command]
#[specta::specta]
pub async fn set_heartbeat(
    state: State<'_, AppState>,
    value: serde_json::Value,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::config::set_config(&*rt.store, "heartbeat_interval", value)
        .await
        .map_err(|e| e.to_string())
}
