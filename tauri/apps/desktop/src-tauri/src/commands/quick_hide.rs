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

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_specta::Event;

/// Emitted from the OS-wide quick-hide shortcut handler. Unit payload —
/// the frontend reacts by collapsing surfaces and (via separate IPC)
/// can lock the vault.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct QuickHide;

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
#[specta::specta]
pub async fn apply_quick_hide_shortcut(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<String, String> {
    let guard = crate::runtime_guard::require_runtime(&state.runtime).await?;
    let rt = guard.as_ref().unwrap();
    let config = springtale_runtime::operations::safety::get_safety_config(rt)
        .await
        .map_err(|e| e.to_string())?;
    drop(guard);

    let configured = config.quick_hide_shortcut.clone();

    // A global shortcut is a CONVENIENCE, not a requirement
    // (https://v2.tauri.app/plugin/global-shortcut/). On macOS,
    // `RegisterEventHotKey` fails when another app already owns the combo —
    // that must never be fatal or block the UI. Strategy (per the Tauri
    // guidance): try the user's combo, then progressively-less-likely-to-
    // conflict fallbacks, and degrade gracefully if none take — the
    // in-window listener still hides on focus, and the user can rebind in
    // Settings → Safety. Returns the combo that actually registered, or an
    // empty string if none did.
    let mut candidates = vec![configured.clone()];
    for fb in ["Alt+Shift+H", "Ctrl+Shift+J", "Ctrl+Alt+Shift+H"] {
        if fb != configured {
            candidates.push(fb.to_owned());
        }
    }

    // Drop whatever was bound before trying new combos (idempotent re-apply).
    let active = app.state::<ActiveQuickHide>();
    let previous = {
        let mut lock = active.0.lock().map_err(|e| e.to_string())?;
        lock.take()
    };
    if let Some(prev) = previous
        && let Err(e) = app.global_shortcut().unregister(prev)
    {
        tracing::warn!(error = %e, "unregister previous quick-hide failed");
    }

    for cand in &candidates {
        let Ok(shortcut) = cand.parse::<Shortcut>() else {
            tracing::warn!(shortcut = %cand, "quick-hide: unparseable shortcut, skipping");
            continue;
        };
        let app_for_handler = app.clone();
        let result = app
            .global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    // Hide immediately for instant survivor feedback; the
                    // runtime-touching teardown runs on the frontend's
                    // "quick-hide" event handler (shared lock-vault flow).
                    if let Some(window) = app_for_handler.get_webview_window("main")
                        && let Err(e) = window.hide()
                    {
                        tracing::warn!(error = %e, "quick-hide window.hide failed");
                    }
                    if let Err(e) = QuickHide.emit(&app_for_handler) {
                        tracing::warn!(error = %e, "quick-hide event emit failed");
                    }
                }
            });
        match result {
            Ok(()) => {
                if let Ok(mut lock) = active.0.lock() {
                    *lock = Some(shortcut);
                }
                if cand != &configured {
                    tracing::warn!(
                        configured = %configured,
                        fallback = %cand,
                        "quick-hide: configured shortcut unavailable — registered fallback"
                    );
                }
                return Ok(cand.clone());
            }
            Err(e) => {
                tracing::warn!(shortcut = %cand, error = %e, "quick-hide: shortcut unavailable");
            }
        }
    }

    // None registered — non-fatal. Empty string tells the UI to degrade
    // quietly (no error banner); the in-window listener still covers focus.
    tracing::warn!("quick-hide: no global shortcut could be registered (all in use)");
    Ok(String::new())
}
