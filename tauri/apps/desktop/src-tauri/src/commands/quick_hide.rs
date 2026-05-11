//! G5g quick-hide — register the persisted `quick_hide_shortcut`
//! as an OS-wide global hotkey so a survivor can hide the window
//! and lock the vault from anywhere on their desktop, not just
//! when Springtale already has focus.
//!
//! Research basis (Tauri 2.10, May 2026):
//! - `tauri_plugin_global_shortcut` exposes `GlobalShortcutExt`
//!   with `on_shortcut(shortcut, handler)` + `unregister(shortcut)`.
//! - `Shortcut::from_str("Ctrl+Shift+Q")` parses the canonical
//!   `modifier+modifier+key` form the Safety panel writes.
//! - The plugin is already registered in `lib.rs::run`'s builder
//!   chain; this module just adds + swaps the actual hotkey.
//!
//! The in-window keydown listener in `apps/desktop/src/App.tsx`
//! stays as a fallback for the pre-unlock window — the global
//! shortcut needs the runtime to load the persisted string, so
//! before the survivor first unlocks the vault, only the window-
//! focused path works. After unlock the global hotkey takes over.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Stores the currently registered shortcut so we can unregister
/// it before swapping. `Mutex<Option<Shortcut>>` because Shortcut
/// doesn't implement Default and we want a single owner.
pub struct ActiveQuickHide(pub Mutex<Option<Shortcut>>);

impl Default for ActiveQuickHide {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

/// Register the persisted quick-hide shortcut as a global hotkey.
/// Reads `SafetyConfig.quick_hide_shortcut`, parses it, and binds
/// it via the global-shortcut plugin. Replaces any previously
/// registered shortcut atomically so this is idempotent.
///
/// On trigger: locks the vault and hides the main window. Both
/// actions are best-effort — failure to hide must not block the
/// vault lock, and failure to lock must not block the hide.
#[tauri::command]
pub async fn apply_quick_hide_shortcut<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<String, String> {
    let guard = crate::runtime_guard::require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let config = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;
    drop(guard);

    let shortcut_str = config.quick_hide_shortcut.clone();
    let shortcut: Shortcut = shortcut_str
        .parse()
        .map_err(|e| format!("invalid shortcut '{shortcut_str}': {e}"))?;

    // Atomically unregister the previous binding (if any) before
    // installing the new one. Swap order matters: register first
    // would risk two handlers firing on the same key during the
    // brief overlap.
    let active = app.state::<ActiveQuickHide>();
    let previous = {
        let mut lock = active.0.lock().map_err(|e| e.to_string())?;
        lock.replace(shortcut)
    };
    if let Some(prev) = previous
        && let Err(e) = app.global_shortcut().unregister(prev)
    {
        tracing::warn!(error = %e, "unregister previous quick-hide failed");
    }

    let app_for_handler = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                // Hide the window immediately for instant survivor
                // feedback — anything that talks to the runtime
                // happens on the frontend's "quick-hide" event
                // handler so we share its lock-vault teardown flow
                // instead of duplicating the multi-step sequence
                // in `commands::vault::lock_vault`.
                if let Some(window) = app_for_handler.get_webview_window("main")
                    && let Err(e) = window.hide()
                {
                    tracing::warn!(error = %e, "quick-hide window.hide failed");
                }
                if let Err(e) = app_for_handler.emit("quick-hide", ()) {
                    tracing::warn!(error = %e, "quick-hide event emit failed");
                }
            }
        })
        .map_err(|e| format!("register quick-hide '{shortcut_str}': {e}"))?;

    Ok(shortcut_str)
}
