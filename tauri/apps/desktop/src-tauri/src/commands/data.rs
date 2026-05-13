use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Export all data.
#[tauri::command]
#[specta::specta]
pub async fn export_data(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let data = springtale_runtime::operations::data::export_data(&*rt.store)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(data).map_err(|e| e.to_string())
}
