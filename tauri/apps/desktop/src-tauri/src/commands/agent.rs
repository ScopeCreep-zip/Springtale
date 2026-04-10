use tauri::State;

use crate::state::AppState;

/// List aggregated agent states (rules + events + autonomy).
#[tauri::command]
pub async fn list_agent_states(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::agent::AgentState>, String> {
    springtale_runtime::operations::agent::list_agent_states(&state.runtime)
        .await
        .map_err(|e| e.to_string())
}

/// Get agent autonomy level.
#[tauri::command]
pub async fn get_autonomy(
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    springtale_runtime::operations::agent::get_autonomy(&*state.runtime.store, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Set agent autonomy level.
#[tauri::command]
pub async fn set_autonomy(
    state: State<'_, AppState>,
    name: String,
    level: String,
) -> Result<(), String> {
    springtale_runtime::operations::agent::set_autonomy(&*state.runtime.store, &name, &level)
        .await
        .map_err(|e| e.to_string())
}

/// Step agent autonomy up or down. Returns the new level.
#[tauri::command]
pub async fn step_autonomy(
    state: State<'_, AppState>,
    name: String,
    direction: springtale_runtime::operations::agent::AutonomyDirection,
) -> Result<String, String> {
    springtale_runtime::operations::agent::step_autonomy(&*state.runtime.store, &name, direction)
        .await
        .map_err(|e| e.to_string())
}
