use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Create a new formation (swarm).
#[tauri::command]
pub async fn create_formation(
    state: State<'_, AppState>,
    name: String,
    intent: String,
    connectors: Vec<String>,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::create_formation(rt, name, intent, connectors)
        .await
        .map_err(|e| e.to_string())
}

/// Deploy a formation.
#[tauri::command]
pub async fn deploy_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::deploy_formation(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Pause a formation.
#[tauri::command]
pub async fn pause_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::pause_formation(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Resume a paused formation.
#[tauri::command]
pub async fn resume_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::resume_formation(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Dissolve a formation.
#[tauri::command]
pub async fn dissolve_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::dissolve_formation(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Update formation intent.
#[tauri::command]
pub async fn update_formation_intent(state: State<'_, AppState>, id: String, intent: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::update_intent(rt, &id, &intent)
        .await
        .map_err(|e| e.to_string())
}

/// Manually trigger self-rally for a formation.
#[tauri::command]
pub async fn rally_formation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::rally_formation(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Add a member to a formation.
#[tauri::command]
pub async fn add_formation_member(state: State<'_, AppState>, formation_id: String, connector_name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::add_member(rt, &formation_id, &connector_name)
        .await
        .map_err(|e| e.to_string())
}

/// Get a single formation with enriched member details.
#[tauri::command]
pub async fn get_formation(
    state: State<'_, AppState>,
    id: String,
) -> Result<springtale_runtime::operations::formations::FormationDetail, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::get_formation(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// List all formations.
#[tauri::command]
pub async fn list_formations(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_runtime::operations::formations::FormationInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::list_formations(rt)
        .await
        .map_err(|e| e.to_string())
}

/// List valid formation intents.
#[tauri::command]
pub async fn list_intents() -> Vec<springtale_runtime::operations::formations::IntentInfo> {
    springtale_runtime::operations::formations::list_intents()
}

/// Backend-supplied formation 3×3 command grid with status-aware enable/disable.
/// Frontend renders the list as-is and dispatches by `id`.
#[tauri::command]
pub async fn formation_commands(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<springtale_runtime::operations::commands::CommandDecl>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::commands::formation_available_commands(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Backend-supplied eligible-removal list for the RM MBR overlay. Backend
/// decides which members are removable; frontend just renders.
#[tauri::command]
pub async fn formation_eligible_members(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<springtale_runtime::operations::commands::MemberRef>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::commands::formation_eligible_members(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Deploy a complete team — creates rules + formation atomically.
#[tauri::command]
pub async fn deploy_team(
    state: State<'_, AppState>,
    team: springtale_runtime::operations::formations::TeamDeployRequest,
) -> Result<springtale_runtime::operations::formations::TeamDeployResult, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::deploy_team(rt, team)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a member from a formation.
#[tauri::command]
pub async fn remove_formation_member(state: State<'_, AppState>, formation_id: String, connector_name: String) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::remove_member(rt, &formation_id, &connector_name)
        .await
        .map_err(|e| e.to_string())
}

/// Cycle a formation's intent to the next in progression.
#[tauri::command]
pub async fn cycle_formation_intent(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::cycle_intent(rt, &id)
        .await
        .map_err(|e| e.to_string())
}

/// Cycle a formation's autonomy to the next level.
#[tauri::command]
pub async fn cycle_formation_autonomy(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::formations::cycle_autonomy(rt, &id)
        .await
        .map_err(|e| e.to_string())
}
