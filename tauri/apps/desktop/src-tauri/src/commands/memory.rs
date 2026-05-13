use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Audit bot memory.
#[tauri::command]
#[specta::specta]
pub async fn audit_memory(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let result = springtale_runtime::operations::memory::audit_memory(&*rt.store)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Compact bot memory.
#[tauri::command]
#[specta::specta]
pub async fn compact_memory(
    state: State<'_, AppState>,
    max_entries: usize,
) -> Result<u64, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::memory::compact_memory(&*rt.store, max_entries)
        .await
        .map_err(|e| e.to_string())
}
