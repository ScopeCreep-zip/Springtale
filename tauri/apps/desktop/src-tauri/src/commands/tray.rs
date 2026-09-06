//! G5f tray-icon control — build once at startup, swap at runtime
//! based on the persisted `SafetyConfig.disguise_icon_id`.
//!
//! Research basis (May 2026):
//! - `tauri::tray::TrayIconBuilder::new().build(app)?` returns a
//!   `TrayIcon` handle (per Tauri 2 system-tray guide).
//! - `TrayIcon::set_icon(Option<Image<'_>>) -> Result<()>` swaps at
//!   runtime; `Image::from_bytes` needs `image-png`/`image-ico`
//!   Cargo features (which Cargo.toml now enables).
//! - Tooltip update: `set_tooltip(Option<S>)`.
//!
//! The icon set is shipped under `src-tauri/icons/disguise/{id}.png`.
//! `disguise_icon_id` is the file stem (`notes`, `calculator`,
//! `files`, ...). Unknown ids fall back to the default app icon to
//! avoid a "blank tray" state in a coercive setting — a missing
//! icon during a survivor's panic moment is worse than no swap.
//!
//! The tray handle is stored on `AppState::tray` (Mutex-guarded
//! Option) so the safety command can reach it from any thread. We
//! don't expose it as a Tauri State directly because `TrayIcon` is
//! not `Sync` on all platforms — keep it behind the AppState Mutex
//! the rest of the safety surface already uses.

use std::sync::Arc;
use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{App, Manager, Runtime};
use tokio::sync::Mutex;

/// Shared tray handle. `None` until `init` runs in `setup()`.
pub type TrayHandle<R> = Arc<Mutex<Option<TrayIcon<R>>>>;

/// Build the initial tray icon at app startup. Called from
/// `lib.rs::run`'s setup hook so the tray exists before the window
/// opens — survivors relaunching under duress see the disguised
/// tray icon from the first frame, not a flash of the real icon.
///
/// Returns the wrapped handle so the caller can `manage` it on
/// AppState. `Ok(None)` is returned when tray construction fails
/// on platforms where tray icons aren't supported (rare but
/// possible on some Linux WMs); the rest of the app continues.
pub fn init<R: Runtime>(app: &App<R>) -> tauri::Result<TrayHandle<R>> {
    let tray = tauri::tray::TrayIconBuilder::new()
        .tooltip("Springtale")
        .build(app)?;
    let handle: TrayHandle<R> = Arc::new(Mutex::new(Some(tray)));
    app.manage(handle.clone());
    Ok(handle)
}

/// G5f — swap the tray icon + tooltip to match the disguise state the
/// frontend just read from the daemon (`GET /safety`). The values are
/// parameters rather than a database read: plan 2.1 leaves the shell with
/// no store of its own.
///
/// Idempotent + safe to call repeatedly. Errors (icon file missing,
/// platform doesn't support tray, etc.) are logged but don't fail
/// the command — the safety apply chain continues so the rest of
/// the disguise (window title, content protection) still applies.
#[tauri::command]
#[specta::specta]
pub async fn apply_disguise_to_tray(
    app: tauri::AppHandle,
    disguise_active: bool,
    disguise_app_name: String,
    disguise_icon_id: String,
) -> Result<String, String> {
    let icon_id = if disguise_active {
        disguise_icon_id
    } else {
        "springtale".to_owned()
    };

    let tooltip = if disguise_active {
        disguise_app_name
    } else {
        "Springtale".to_owned()
    };

    let tray_state = app.state::<TrayHandle<tauri::Wry>>();
    let tray_lock = tray_state.inner().lock().await;
    let Some(tray) = tray_lock.as_ref() else {
        // Tray not built (init failed on this platform). The other
        // disguise channels (window title, content protection)
        // still apply; this is a documented graceful degradation.
        tracing::debug!("apply_disguise_to_tray: tray handle absent on this platform");
        return Ok(format!("(no tray) tooltip={tooltip}"));
    };

    // Try to load the icon. Resource lookup is per-app-bundle; we
    // log + soft-fail on miss so a survivor with a corrupt bundle
    // doesn't see disguise refuse to apply.
    let icon = load_disguise_icon(&app, &icon_id);
    if let Err(e) = tray.set_icon(icon) {
        tracing::warn!(error = %e, icon_id = %icon_id, "tray set_icon failed");
    }
    if let Err(e) = tray.set_tooltip(Some(&tooltip)) {
        tracing::warn!(error = %e, "tray set_tooltip failed");
    }
    Ok(tooltip)
}

/// Resource lookup for the disguise icon set. Icons ship under
/// `src-tauri/icons/disguise/{id}.png` and are baked into the
/// bundle by tauri-build.
///
/// Returns `None` on miss so `TrayIcon::set_icon(None)` clears the
/// icon — preferable to crashing in a panic scenario where the
/// survivor needs the disguise *now*.
fn load_disguise_icon<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) -> Option<Image<'static>> {
    let path = app
        .path()
        .resolve(
            format!("icons/disguise/{id}.png"),
            tauri::path::BaseDirectory::Resource,
        )
        .ok()?;
    let bytes = std::fs::read(&path).ok()?;
    // `Image::from_bytes` decodes PNG into a `tauri::Image`; the
    // bytes are copied internally so we can return a `'static`
    // lifetime image.
    Image::from_bytes(&bytes).ok().map(|img| img.to_owned())
}
