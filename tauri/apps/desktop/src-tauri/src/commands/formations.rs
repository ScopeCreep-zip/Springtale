use tauri::State;

use crate::state::AppState;

/// Create a new formation (swarm).
#[tauri::command]
pub async fn create_formation(
    state: State<'_, AppState>,
    name: String,
    intent: String,
    connectors: Vec<String>,
) -> Result<String, String> {
    springtale_runtime::operations::formations::create_formation(&state.runtime, name, intent, connectors)
        .await
        .map_err(|e| e.to_string())
}

/// Deploy a formation.
#[tauri::command]
pub async fn deploy_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    springtale_runtime::operations::formations::deploy_formation(&state.runtime, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Pause a formation.
#[tauri::command]
pub async fn pause_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    springtale_runtime::operations::formations::pause_formation(&state.runtime, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Resume a paused formation.
#[tauri::command]
pub async fn resume_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    springtale_runtime::operations::formations::resume_formation(&state.runtime, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Dissolve a formation.
#[tauri::command]
pub async fn dissolve_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    springtale_runtime::operations::formations::dissolve_formation(&state.runtime, &id)
        .await
        .map_err(|e| e.to_string())
}

/// List all formations.
#[tauri::command]
pub async fn list_formations(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::formations::FormationInfo>, String> {
    springtale_runtime::operations::formations::list_formations(&state.runtime)
        .await
        .map_err(|e| e.to_string())
}
