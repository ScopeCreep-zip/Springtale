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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfigRow {
    pub window_title: String,
    pub auto_lock_minutes: u32,
    pub content_protected: bool,
    pub quick_hide_shortcut: String,
    pub updated_at: DateTime<Utc>,
}

impl Default for SafetyConfigRow {
    fn default() -> Self {
        Self {
            window_title: "Notes".to_owned(),
            auto_lock_minutes: 5,
            content_protected: true,
            quick_hide_shortcut: "Ctrl+Shift+H".to_owned(),
            updated_at: Utc::now(),
        }
    }
}
