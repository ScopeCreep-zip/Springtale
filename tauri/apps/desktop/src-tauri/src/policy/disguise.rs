//! Which name and icon the shell presents.
//!
//! The daemon stores the disguise config; the frontend reads it and hands
//! the fields to `commands::safety::apply_disguise_to_shell` and
//! `commands::tray::apply_disguise_to_tray`. Both commands do exactly two
//! things: pick the values below, then push them at the OS. This module is
//! the picking half.
//!
//! The window title and the tray tooltip are chosen independently of
//! whether the OS accepts them — a survivor's disguise must not depend on a
//! window manager that refuses a tray icon.

/// Tray icon stem used when disguise is off. Icons ship as
/// `src-tauri/icons/disguise/{id}.png`.
pub const REAL_TRAY_ICON_ID: &str = "springtale";

/// Tray tooltip used when disguise is off.
pub const REAL_TRAY_TOOLTIP: &str = "Springtale";

/// The tray half of a disguise decision: which icon to load and what the
/// hover text says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayDisguise {
    /// File stem under `icons/disguise/`. An unknown id resolves to no
    /// icon at load time rather than failing the disguise.
    pub icon_id: String,
    /// Tooltip text shown on hover.
    pub tooltip: String,
}

/// Title to put on the main window.
///
/// Disguise on: the cover app's name. Disguise off: the configured
/// `window_title`, which itself defaults to the disguise-friendly "Notes"
/// per the IPV-first defaults — "off" never means "announce Springtale".
#[must_use]
pub fn select_window_title(
    disguise_active: bool,
    disguise_app_name: String,
    window_title: String,
) -> String {
    if disguise_active {
        disguise_app_name
    } else {
        window_title
    }
}

/// Icon + tooltip for the tray.
///
/// Unlike the window title, the undisguised tray is the real product: the
/// tray is where someone looks to see whether Springtale is running at all.
#[must_use]
pub fn select_tray_disguise(
    disguise_active: bool,
    disguise_app_name: String,
    disguise_icon_id: String,
) -> TrayDisguise {
    if disguise_active {
        TrayDisguise {
            icon_id: disguise_icon_id,
            tooltip: disguise_app_name,
        }
    } else {
        TrayDisguise {
            icon_id: REAL_TRAY_ICON_ID.to_owned(),
            tooltip: REAL_TRAY_TOOLTIP.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{REAL_TRAY_ICON_ID, REAL_TRAY_TOOLTIP, select_tray_disguise, select_window_title};

    #[test]
    fn test_select_window_title_disguise_active_uses_app_name() {
        assert_eq!(
            select_window_title(true, "Calculator".to_owned(), "Notes".to_owned()),
            "Calculator"
        );
    }

    #[test]
    fn test_select_window_title_disguise_inactive_uses_configured_title() {
        assert_eq!(
            select_window_title(false, "Calculator".to_owned(), "Notes".to_owned()),
            "Notes"
        );
    }

    #[test]
    fn test_select_window_title_inactive_never_leaks_the_disguise_name() {
        // The cover name must not appear when the survivor turned disguise
        // off — the two fields are stored independently.
        assert_eq!(
            select_window_title(false, "Calculator".to_owned(), String::new()),
            ""
        );
    }

    #[test]
    fn test_select_tray_disguise_active_uses_configured_icon_and_name() {
        let profile = select_tray_disguise(true, "Files".to_owned(), "files".to_owned());
        assert_eq!(profile.icon_id, "files");
        assert_eq!(profile.tooltip, "Files");
    }

    #[test]
    fn test_select_tray_disguise_inactive_restores_the_real_identity() {
        let profile = select_tray_disguise(false, "Files".to_owned(), "files".to_owned());
        assert_eq!(profile.icon_id, REAL_TRAY_ICON_ID);
        assert_eq!(profile.tooltip, REAL_TRAY_TOOLTIP);
    }

    #[test]
    fn test_select_tray_disguise_unknown_icon_id_is_passed_through() {
        // Resolution happens at load time, where a miss degrades to "no
        // icon" — the selection stage must not second-guess the id.
        let profile = select_tray_disguise(true, "Weather".to_owned(), "not-a-real-id".to_owned());
        assert_eq!(profile.icon_id, "not-a-real-id");
    }
}
