use serde::{Deserialize, Serialize};
use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Safety configuration — IPC presentation type.
#[derive(Serialize, Deserialize, Clone)]
pub struct SafetyConfig {
    pub window_title: String,
    pub auto_lock_minutes: u32,
    pub content_protected: bool,
    pub quick_hide_shortcut: String,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            window_title: "Notes".to_owned(),
            auto_lock_minutes: 5,
            content_protected: true,
            quick_hide_shortcut: "Ctrl+Shift+H".to_owned(),
        }
    }
}

impl From<springtale_store::SafetyConfigRow> for SafetyConfig {
    fn from(row: springtale_store::SafetyConfigRow) -> Self {
        Self {
            window_title: row.window_title,
            auto_lock_minutes: row.auto_lock_minutes,
            content_protected: row.content_protected,
            quick_hide_shortcut: row.quick_hide_shortcut,
        }
    }
}

/// Get the current safety configuration.
#[tauri::command]
pub async fn get_safety_config(
    state: State<'_, AppState>,
) -> Result<SafetyConfig, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let row = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SafetyConfig::from(row))
}

/// Save safety configuration.
#[tauri::command]
pub async fn save_safety_config(
    state: State<'_, AppState>,
    config: SafetyConfig,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let row = springtale_store::SafetyConfigRow {
        window_title: config.window_title,
        auto_lock_minutes: config.auto_lock_minutes,
        content_protected: config.content_protected,
        quick_hide_shortcut: config.quick_hide_shortcut,
        updated_at: chrono::Utc::now(),
    };
    springtale_runtime::operations::safety::save_safety_config(rt, row)
        .await
        .map_err(|e| e.to_string())
}

/// Set the window title — desktop-specific (Tauri API).
#[tauri::command]
pub async fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

/// Reset the auto-lock timer — desktop-specific.
#[tauri::command]
pub async fn reset_auto_lock(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let config = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;

    let minutes = config.auto_lock_minutes;

    let mut handle = state.auto_lock.lock().await;
    handle.reset(minutes, state.vault.clone(), app);
    Ok(())
}
