use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// List aggregated agent states (rules + events + autonomy).
#[tauri::command]
#[specta::specta]
pub async fn list_agent_states(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::agent::AgentState>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::agent::list_agent_states(rt)
        .await
        .map_err(|e| e.to_string())
}

/// Get agent autonomy level.
#[tauri::command]
#[specta::specta]
pub async fn get_autonomy(
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::agent::get_autonomy(&*rt.store, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Set agent autonomy level.
#[tauri::command]
#[specta::specta]
pub async fn set_autonomy(
    state: State<'_, AppState>,
    name: String,
    level: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::agent::set_autonomy(&*rt.store, &name, &level)
        .await
        .map_err(|e| e.to_string())
}

/// Step agent autonomy up or down. Returns the new level.
#[tauri::command]
#[specta::specta]
pub async fn step_autonomy(
    state: State<'_, AppState>,
    name: String,
    direction: springtale_runtime::operations::agent::AutonomyDirection,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::agent::step_autonomy(&*rt.store, &name, direction)
        .await
        .map_err(|e| e.to_string())
}
