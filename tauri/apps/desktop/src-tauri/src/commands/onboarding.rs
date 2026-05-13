use std::collections::BTreeMap;

use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;
use springtale_runtime::operations::onboarding::{self, ApplyReport, PlatformForm};

/// List all onboarding platform forms (wizard step definitions).
#[tauri::command]
#[specta::specta]
pub async fn list_onboarding_platforms() -> Result<Vec<PlatformForm>, String> {
    Ok(onboarding::list_platforms().into_iter().cloned().collect())
}

/// Apply an onboarding wizard answer set — persist connector config.
#[tauri::command]
#[specta::specta]
pub async fn apply_onboarding(
    state: State<'_, AppState>,
    platform: String,
    answers: BTreeMap<String, String>,
) -> Result<ApplyReport, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    onboarding::apply_platform(&*rt.store, &platform, answers)
        .await
        .map_err(|e| e.to_string())
}
