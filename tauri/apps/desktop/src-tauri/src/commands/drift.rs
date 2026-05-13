//! Drift detection IPC.
//!
//! Thin: validates args, defers to
//! `springtale-runtime::operations::executions::drift` for the
//! latency / success / refusal-rate trend analysis. Returns the
//! `DriftReport` directly — privacy-shaped (sizes / counts /
//! enum tags only) per the executions-log invariant.

use tauri::State;

use springtale_runtime::operations::executions::{
    recipe_drift, rule_drift, DriftFilter, DriftReport,
};

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn get_recipe_drift(
    state: State<'_, AppState>,
    recipe_id: String,
    filter: DriftFilter,
) -> Result<DriftReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    recipe_drift(&rt.store, &recipe_id, filter)
        .await
        .map_err(|e| format!("get_recipe_drift: {e}"))
}

#[tauri::command]
#[specta::specta]
pub async fn get_rule_drift(
    state: State<'_, AppState>,
    filter: DriftFilter,
) -> Result<DriftReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    rule_drift(&rt.store, filter)
        .await
        .map_err(|e| format!("get_rule_drift: {e}"))
}
