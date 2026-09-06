//! Bot settings commands (plan 6.3) — persona, context window and the AI
//! tool allow-list, edited from the settings panel instead of a TOML file.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use crate::runtime_guard::require_runtime;
use crate::state::AppState;

use springtale_runtime::operations::bot_settings::BotSettings;

/// Read the current bot settings (defaults when never saved).
#[tauri::command]
pub async fn get_bot_settings(state: State<'_, AppState>) -> Result<BotSettings, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::bot_settings::get(&*rt.store)
        .await
        .map_err(|e| e.to_string())
}

/// Save bot settings. The operation validates every literal tool in the
/// allow-list against the connector registry before it writes.
#[tauri::command]
pub async fn save_bot_settings(
    state: State<'_, AppState>,
    settings: BotSettings,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::bot_settings::set(rt, settings)
        .await
        .map_err(|e| e.to_string())
}
