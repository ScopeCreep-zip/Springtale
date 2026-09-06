//! Shell preferences that must be readable before the vault is unlocked.
//!
//! Everything Springtale persists lives in the daemon's encrypted database,
//! which by design cannot be read until the survivor types their passphrase.
//! That is the right default for their data — and the wrong one for the
//! window title. Someone who set the app to present itself as "Notes" needs
//! it to say "Notes" on the very first frame of a cold start, before any
//! unlock, because the moment the disguise is most needed is the moment
//! someone else is looking at the screen.
//!
//! So the one value that has to survive a cold start is mirrored here, in a
//! small plaintext JSON file next to the config: the resolved window title,
//! and nothing else. A window title is not a secret — it is drawn on screen
//! by definition. The database stays the source of truth; this file is a
//! write-through cache updated every time `apply_disguise_to_shell` runs.
//!
//! The file is still `0600`: which app a person is disguising, and as what,
//! is not something other accounts on a shared machine should be able to
//! enumerate.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

/// The subset of shell state that outlives a locked vault.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShellPrefs {
    /// Last title applied to the main window — the disguise name when
    /// disguise is active, otherwise the configured `window_title`.
    #[serde(default)]
    pub window_title: Option<String>,
}

/// Path of the preferences file: `{data_dir}/shell-prefs.json`.
#[must_use]
pub fn shell_prefs_path() -> PathBuf {
    crate::paths::data_dir().join("shell-prefs.json")
}

/// Read the preferences, falling back to defaults on any error.
///
/// A missing, unreadable or malformed file must never block startup: the
/// window still opens with the title from `tauri.conf.json`, which is itself
/// the disguise-friendly default.
#[must_use]
pub fn load() -> ShellPrefs {
    std::fs::read_to_string(shell_prefs_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Persist the applied window title so the next cold start can use it.
pub fn save_window_title(title: &str) -> Result<(), String> {
    let path = shell_prefs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let prefs = ShellPrefs {
        window_title: Some(title.to_owned()),
    };
    let body = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Apply the remembered window title during `setup`, before the frontend
/// has run and long before the daemon exists.
pub fn apply_window_title<R: Runtime>(app: &tauri::App<R>) {
    let Some(title) = load().window_title else {
        return;
    };
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Err(e) = window.set_title(&title) {
        tracing::warn!(error = %e, "cold-start window title could not be applied");
    }
}

#[cfg(test)]
mod tests {
    use super::ShellPrefs;

    #[test]
    fn test_shell_prefs_round_trips_the_window_title() {
        let json = serde_json::to_string(&ShellPrefs {
            window_title: Some("Notes".to_owned()),
        })
        .expect("serialize");
        let back: ShellPrefs = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.window_title.as_deref(), Some("Notes"));
    }

    #[test]
    fn test_shell_prefs_empty_object_yields_no_title() {
        let prefs: ShellPrefs = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(prefs.window_title, None);
    }
}
