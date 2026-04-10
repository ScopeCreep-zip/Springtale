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

/// Update formation intent.
#[tauri::command]
pub async fn update_formation_intent(state: State<'_, AppState>, id: String, intent: String) -> Result<(), String> {
    springtale_runtime::operations::formations::update_intent(&state.runtime, &id, &intent)
        .await
        .map_err(|e| e.to_string())
}

/// Add a member to a formation.
#[tauri::command]
pub async fn add_formation_member(state: State<'_, AppState>, formation_id: String, connector_name: String) -> Result<(), String> {
    springtale_runtime::operations::formations::add_member(&state.runtime, &formation_id, &connector_name)
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

/// List valid formation intents.
#[tauri::command]
pub async fn list_intents() -> Vec<springtale_runtime::operations::formations::IntentInfo> {
    springtale_runtime::operations::formations::list_intents()
}

/// Deploy a complete team — creates rules + formation atomically.
#[tauri::command]
pub async fn deploy_team(
    state: State<'_, AppState>,
    team: springtale_runtime::operations::formations::TeamDeployRequest,
) -> Result<springtale_runtime::operations::formations::TeamDeployResult, String> {
    springtale_runtime::operations::formations::deploy_team(&state.runtime, team)
        .await
        .map_err(|e| e.to_string())
}

/// Cycle a formation's intent to the next in progression.
#[tauri::command]
pub async fn cycle_formation_intent(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    springtale_runtime::operations::formations::cycle_intent(&state.runtime, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Cycle a formation's autonomy to the next level.
#[tauri::command]
pub async fn cycle_formation_autonomy(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    springtale_runtime::operations::formations::cycle_autonomy(&state.runtime, &id)
        .await
        .map_err(|e| e.to_string())
}
