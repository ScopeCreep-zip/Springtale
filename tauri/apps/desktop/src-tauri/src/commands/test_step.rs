//! Test This Step IPC.
//!
//! Thin: validates args, defers to
//! `springtale-runtime::operations::test_step::test_recipe_step`
//! for everything. The runtime runs the chain in DryRun mode
//! through the targeted step, then returns the recorded output.

use tauri::State;

use springtale_runtime::operations::recipes::types::RecipeInputs;
use springtale_runtime::operations::test_step::TestStepReport;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn test_recipe_step(
    state: State<'_, AppState>,
    recipe_id: String,
    inputs: RecipeInputs,
    rule_index: usize,
    step_index: usize,
) -> Result<TestStepReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::test_step::test_recipe_step(
        rt, &recipe_id, inputs, rule_index, step_index,
    )
    .await
    .map_err(|e| format!("test_recipe_step: {e}"))
}
