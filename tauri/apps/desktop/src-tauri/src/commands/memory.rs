use tauri::State;

use crate::state::AppState;

/// Audit bot memory.
#[tauri::command]
pub async fn audit_memory(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let result = springtale_runtime::operations::memory::audit_memory(&*state.runtime.store)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Compact bot memory.
#[tauri::command]
pub async fn compact_memory(
    state: State<'_, AppState>,
    max_entries: usize,
) -> Result<u64, String> {
    springtale_runtime::operations::memory::compact_memory(&*state.runtime.store, max_entries)
        .await
        .map_err(|e| e.to_string())
}
