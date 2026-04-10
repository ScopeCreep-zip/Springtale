use tauri::State;

use crate::state::AppState;

/// Export all data.
#[tauri::command]
pub async fn export_data(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let data = springtale_runtime::operations::data::export_data(&*state.runtime.store)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(data).map_err(|e| e.to_string())
}
