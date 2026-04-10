use tauri::State;

use crate::state::AppState;

/// Get a config value by key.
#[tauri::command]
pub async fn get_config(
    state: State<'_, AppState>,
    key: String,
) -> Result<serde_json::Value, String> {
    springtale_runtime::operations::config::get_config(&*state.runtime.store, &key)
        .await
        .map_err(|e| e.to_string())
}

/// Set a config value (upsert).
#[tauri::command]
pub async fn set_config(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    springtale_runtime::operations::config::set_config(&*state.runtime.store, &key, value)
        .await
        .map_err(|e| e.to_string())
}

/// List all config entries.
#[tauri::command]
pub async fn list_config(
    state: State<'_, AppState>,
) -> Result<Vec<(String, serde_json::Value)>, String> {
    springtale_runtime::operations::config::list_config(&*state.runtime.store)
        .await
        .map_err(|e| e.to_string())
}

/// Set AI adapter config and hot-swap at runtime.
#[tauri::command]
pub async fn set_ai_adapter(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> Result<(), String> {
    springtale_runtime::operations::config::set_ai_adapter(&state.runtime, config)
        .await
        .map_err(|e| e.to_string())
}

/// Set connector config (persisted for next load).
#[tauri::command]
pub async fn set_connector_config(
    state: State<'_, AppState>,
    name: String,
    config: serde_json::Value,
) -> Result<(), String> {
    springtale_runtime::operations::config::set_connector_config(&state.runtime, &name, config)
        .await
        .map_err(|e| e.to_string())
}

/// Configure AI adapter — persists under target key and hot-swaps.
#[tauri::command]
pub async fn configure_ai_adapter(
    state: State<'_, AppState>,
    target: String,
    config: serde_json::Value,
) -> Result<(), String> {
    springtale_runtime::operations::config::configure_ai_adapter(&state.runtime, &target, config)
        .await
        .map_err(|e| e.to_string())
}

/// Upsert connector config — setup if new, update if exists.
#[tauri::command]
pub async fn upsert_connector_config(
    state: State<'_, AppState>,
    name: String,
    config: serde_json::Value,
) -> Result<bool, String> {
    springtale_runtime::operations::config::upsert_connector_config(&state.runtime, &name, config)
        .await
        .map_err(|e| e.to_string())
}

/// Toggle guard mode for a formation.
#[tauri::command]
pub async fn toggle_formation_guard(
    state: State<'_, AppState>,
    formation_id: String,
) -> Result<bool, String> {
    springtale_runtime::operations::config::toggle_formation_guard(&state.runtime, &formation_id)
        .await
        .map_err(|e| e.to_string())
}
