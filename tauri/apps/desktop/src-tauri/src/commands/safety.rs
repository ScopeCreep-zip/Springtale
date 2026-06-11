use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::runtime_guard::require_runtime;
use crate::state::AppState;

/// Safety configuration — IPC presentation type.
///
/// **G5d safety bug fix:** earlier versions of this struct were a
/// *subset* of `SafetyConfigRow` (only the four legacy fields). Any
/// round-trip through `get` → user-edit → `save` therefore zeroed the
/// new G5d disguise fields, which would silently flip a survivor's
/// persisted disguise off the next time the Safety panel saved. The
/// struct now mirrors the full `SafetyConfigRow` shape so every field
/// survives a save round-trip. The `updated_at` field is intentionally
/// generated server-side in `save_safety_config`, not threaded through
/// the IPC layer.
#[derive(Serialize, Deserialize, Clone, Type)]
pub struct SafetyConfig {
    pub window_title: String,
    pub auto_lock_minutes: u32,
    pub content_protected: bool,
    pub quick_hide_shortcut: String,
    pub disguise_app_name: String,
    pub disguise_icon_id: String,
    pub disguise_active: bool,
    pub panic_tap_count: u32,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self::from(springtale_store::SafetyConfigRow::default())
    }
}

impl From<springtale_store::SafetyConfigRow> for SafetyConfig {
    fn from(row: springtale_store::SafetyConfigRow) -> Self {
        Self {
            window_title: row.window_title,
            auto_lock_minutes: row.auto_lock_minutes,
            content_protected: row.content_protected,
            quick_hide_shortcut: row.quick_hide_shortcut,
            disguise_app_name: row.disguise_app_name,
            disguise_icon_id: row.disguise_icon_id,
            disguise_active: row.disguise_active,
            panic_tap_count: row.panic_tap_count,
        }
    }
}

/// Get the current safety configuration.
#[tauri::command]
#[specta::specta]
pub async fn get_safety_config(state: State<'_, AppState>) -> Result<SafetyConfig, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let row = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SafetyConfig::from(row))
}

/// Save safety configuration.
#[tauri::command]
#[specta::specta]
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
        disguise_app_name: config.disguise_app_name,
        disguise_icon_id: config.disguise_icon_id,
        disguise_active: config.disguise_active,
        panic_tap_count: config.panic_tap_count,
        updated_at: chrono::Utc::now(),
    };
    springtale_runtime::operations::safety::save_safety_config(rt, row)
        .await
        .map_err(|e| e.to_string())
}

/// G5d — toggle just the disguise-active flag without re-sending the
/// full config. Eliminates the read-modify-write race two tabs would
/// hit on the full-config PUT path.
#[tauri::command]
#[specta::specta]
pub async fn set_disguise_active(state: State<'_, AppState>, active: bool) -> Result<bool, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::safety::set_disguise_active(rt, active)
        .await
        .map_err(|e| e.to_string())
}

/// G5d — atomically update the disguise profile (app name + icon id).
/// Doesn't flip `disguise_active`; profile selection is decoupled
/// from whether the disguise is currently displayed.
#[tauri::command]
#[specta::specta]
pub async fn set_disguise_profile(
    state: State<'_, AppState>,
    app_name: String,
    icon_id: String,
) -> Result<(), String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::safety::set_disguise_profile(rt, app_name, icon_id)
        .await
        .map_err(|e| e.to_string())
}

/// G5d — adjust the panic-tap threshold. `count = 0` disables the
/// gesture; bounded `[0, 10]` server-side so an accidental large
/// value can't render panic-wipe unreachable.
#[tauri::command]
#[specta::specta]
pub async fn set_panic_tap_count(state: State<'_, AppState>, count: u32) -> Result<u32, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    springtale_runtime::operations::safety::set_panic_tap_count(rt, count)
        .await
        .map_err(|e| e.to_string())
}

/// Set the window title — desktop-specific (Tauri API).
#[tauri::command]
#[specta::specta]
pub async fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

/// G5g — apply the persisted `content_protected` flag to the Tauri
/// window. On macOS + Windows this blocks screenshots and screen
/// recording at the OS compositor level (per
/// `docs/intended-arch/ARCHITECTURE.md §2.8`); on Linux most window
/// managers don't expose the compositor flag, so the Tauri call
/// returns an Err there — we log + swallow so the rest of the
/// safety apply chain still proceeds.
///
/// Idempotent: callable from `onMount` + after any
/// `save_safety_config` without state drift. Returns the bool that
/// was actually applied (matches the persisted config).
#[tauri::command]
#[specta::specta]
pub async fn apply_content_protection(
    state: State<'_, AppState>,
    window: tauri::Window,
) -> Result<bool, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let config = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;
    // F — previously this swallowed the error at `tracing::debug!`
    // and returned `Ok(applied_value)` regardless, which made a
    // Linux failure (or any other underlying error) look identical
    // to success from the frontend's perspective — the panel toggle
    // appeared to do nothing and there was no error to debug.
    // Propagate the error so SafetyPanel's `onSafetyChanged`
    // catch routes it to `db.setError` and the user sees it.
    window
        .set_content_protected(config.content_protected)
        .map_err(|e| {
            format!("set_content_protected failed (Linux is unsupported by Tauri 2): {e}")
        })?;
    Ok(config.content_protected)
}

/// G5f — apply the current `disguise_active` + `disguise_app_name`
/// state to the visible shell (window title, future tray icon).
/// Reads the persisted SafetyConfig so the call is idempotent: the
/// frontend invokes this after any disguise-related backend write
/// and the shell snaps to whatever the backend says is current.
///
/// Returns the title that was actually applied so the frontend can
/// surface it in the UI (e.g. confirmation toast). When disguise is
/// active the title is `disguise_app_name`; otherwise the literal
/// `window_title` from the config (which itself defaults to
/// disguise-friendly "Notes" per the IPV-first defaults).
///
/// **Mobile note:** iOS alternate-icons and Android dynamic launcher
/// alias updates are platform-specific Tauri 2 mobile plugin work
/// that still requires Tauri 2 mobile plugin maturity (per
/// `.claude/phases/phase-2b.md` "Research needed" note). When those
/// plugins ship, this command extends to invoke them; the persisted
/// `disguise_icon_id` field is already the input.
#[tauri::command]
#[specta::specta]
pub async fn apply_disguise_to_shell(
    state: State<'_, AppState>,
    window: tauri::Window,
) -> Result<String, String> {
    let guard = require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let config = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;
    let title = if config.disguise_active {
        config.disguise_app_name.clone()
    } else {
        config.window_title.clone()
    };
    window.set_title(&title).map_err(|e| e.to_string())?;
    Ok(title)
}

/// Reset the auto-lock timer — desktop-specific.
#[tauri::command]
#[specta::specta]
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
