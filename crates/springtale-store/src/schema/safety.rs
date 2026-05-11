//! Safety configuration row — persisted outside the vault.
//!
//! Per ARCHITECTURE.md §2.8: safety settings load before vault unlock
//! so the app starts disguised. Stored in SQLite, not in the encrypted vault.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Safety configuration persisted to SQLite.
///
/// Single-row table (id=1). Defaults are safe for IPV scenarios:
/// window title "Notes", auto-lock 5 minutes, content protection on.
///
/// G5d adds the app-disguise fields (`disguise_app_name`,
/// `disguise_icon_id`, `disguise_active`, `panic_tap_count`) so the
/// duress surface survives restart. Migration 012 adds these columns
/// with safe defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfigRow {
    pub window_title: String,
    pub auto_lock_minutes: u32,
    pub content_protected: bool,
    pub quick_hide_shortcut: String,
    /// G5d — app-display name the OS shows in the task switcher /
    /// launcher when disguise is active. Independent from
    /// `window_title` so the in-app titlebar can read "Notes" while
    /// the launcher tile reads, e.g., "Calculator".
    pub disguise_app_name: String,
    /// G5d — opaque identifier of which icon-set the Tauri shell
    /// should display. Actual icons ship as Tauri resources keyed by
    /// this id; the backend only records which one is active.
    pub disguise_icon_id: String,
    /// G5d — whether the app currently renders the disguised UI.
    /// Persisted so the disguise survives a restart of the process
    /// (critical for survivors — opening the app under coercion must
    /// not reveal the real surface).
    pub disguise_active: bool,
    /// G5d — number of rapid title-bar taps that trigger panic-wipe.
    /// Default 5; 0 disables the gesture entirely.
    pub panic_tap_count: u32,
    pub updated_at: DateTime<Utc>,
}

impl Default for SafetyConfigRow {
    fn default() -> Self {
        Self {
            window_title: "Notes".to_owned(),
            auto_lock_minutes: 5,
            content_protected: true,
            quick_hide_shortcut: "Ctrl+Shift+H".to_owned(),
            disguise_app_name: "Notes".to_owned(),
            disguise_icon_id: "notes".to_owned(),
            disguise_active: false,
            panic_tap_count: 5,
            updated_at: Utc::now(),
        }
    }
}
