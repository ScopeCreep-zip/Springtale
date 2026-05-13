//! Executions-log IPC. Thin: validates args, defers to
//! `springtale-runtime::operations::executions` for everything.
//!
//! Privacy-default by design — the runtime layer returns sizes-only
//! rows; this command surface forwards them unchanged. Content
//! retention is a separate, opt-in code path (Phase C).

use tauri::State;

use springtale_runtime::operations::executions::{
    ExecutionFilterIpc, ExecutionInfo, ExecutionStepInfo,
};

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// List recent executions matching `filter`. Newest-first.
/// Pagination cursor on the result's `started_at` — pass
/// `before = oldest_started_at` to fetch the next page.
#[tauri::command]
#[specta::specta]
pub async fn list_executions(
    state: State<'_, AppState>,
    filter: ExecutionFilterIpc,
) -> Result<Vec<ExecutionInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::executions::list_executions_ipc(&rt.store, filter)
        .await
        .map_err(|e| format!("list_executions: {e}"))
}

/// Fetch every step row for one execution. Returns an empty Vec
/// when the execution failed before any step recorded.
#[tauri::command]
#[specta::specta]
pub async fn get_execution_steps(
    state: State<'_, AppState>,
    execution_id: String,
) -> Result<Vec<ExecutionStepInfo>, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::executions::get_execution_steps_ipc(
        &rt.store,
        &execution_id,
    )
    .await
    .map_err(|e| format!("get_execution_steps: {e}"))
}

/// Drop expired rows now (rather than waiting for the next
/// background tick). Exposed for tests + manual triggering;
/// production paths rely on the daemon's vacuum task.
#[tauri::command]
#[specta::specta]
pub async fn vacuum_executions(state: State<'_, AppState>) -> Result<u64, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let now_ms = chrono::Utc::now().timestamp_millis();
    springtale_runtime::operations::executions::vacuum_executions(&rt.store, now_ms)
        .await
        .map_err(|e| format!("vacuum_executions: {e}"))
}
