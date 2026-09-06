//! OS-side of the safety surface.
//!
//! Plan 2.1: the persisted `SafetyConfig` lives in the daemon and is read
//! and written over `GET`/`PUT /safety`. What stays here is what only the
//! desktop shell can do — retitle the window, flip the compositor's
//! content-protection flag, and drive the auto-lock timer. Each command
//! takes the values the frontend just read from the daemon, so the shell
//! never needs a database of its own.

use tauri::State;

use crate::state::AppState;

/// Set the window title — desktop-specific (Tauri API).
#[tauri::command]
#[specta::specta]
pub async fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

/// G5g — apply the `content_protected` flag to the Tauri window.
///
/// On macOS + Windows this blocks screenshots and screen recording at the
/// OS compositor level (per `docs/intended-arch/ARCHITECTURE.md §2.8`); on
/// Linux most window managers don't expose the flag, so Tauri returns an
/// error there. The error is propagated rather than swallowed: a silent
/// "applied" for a protection that did not apply is the worst outcome for
/// someone relying on it.
///
/// Idempotent — safe to call on mount and after every safety write.
/// Returns the flag that was applied.
#[tauri::command]
#[specta::specta]
pub async fn apply_content_protection(
    window: tauri::Window,
    protected: bool,
) -> Result<bool, String> {
    window.set_content_protected(protected).map_err(|e| {
        format!("set_content_protected failed (Linux is unsupported by Tauri 2): {e}")
    })?;
    Ok(protected)
}

/// G5f — apply the disguise state to the visible shell (window title).
///
/// Returns the title that was actually applied so the frontend can surface
/// it. When disguise is active the title is `disguise_app_name`; otherwise
/// the literal `window_title` from the config (which itself defaults to the
/// disguise-friendly "Notes" per the IPV-first defaults).
///
/// **Mobile note:** iOS alternate-icons and Android dynamic launcher alias
/// updates are platform-specific Tauri 2 mobile plugin work. When those
/// plugins ship, this command extends to invoke them; `disguise_icon_id` is
/// already the input.
#[tauri::command]
#[specta::specta]
pub async fn apply_disguise_to_shell(
    window: tauri::Window,
    disguise_active: bool,
    disguise_app_name: String,
    window_title: String,
) -> Result<String, String> {
    let title = if disguise_active {
        disguise_app_name
    } else {
        window_title
    };
    window.set_title(&title).map_err(|e| e.to_string())?;
    // Mirror the applied title into the pre-unlock prefs file so a cold
    // start shows the disguise on its first frame instead of the real name.
    // Non-fatal: the disguise is already applied to the live window, and
    // failing the command here would make the panel look broken.
    if let Err(e) = crate::prefs::save_window_title(&title) {
        tracing::warn!(error = %e, "could not persist window title for cold start");
    }
    Ok(title)
}

/// Reset the auto-lock timer — desktop-specific.
#[tauri::command]
#[specta::specta]
pub async fn reset_auto_lock(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    auto_lock_minutes: u32,
) -> Result<(), String> {
    let mut handle = state.auto_lock.lock().await;
    handle.reset(auto_lock_minutes, state.vault.clone(), app);
    Ok(())
}
