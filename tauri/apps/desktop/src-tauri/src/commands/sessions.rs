use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// List all bot sessions (user/channel memory).
#[tauri::command]
#[specta::specta]
pub async fn list_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<springtale_store::SessionRow>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    rt.store.list_sessions().await.map_err(|e| e.to_string())
}
