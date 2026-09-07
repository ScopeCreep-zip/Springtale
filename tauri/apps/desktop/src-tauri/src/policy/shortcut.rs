//! The quick-hide hotkey ladder.
//!
//! A global shortcut is a convenience, not a guarantee: on macOS
//! `RegisterEventHotKey` fails outright when another application already
//! owns the combo. `commands::quick_hide` therefore tries the survivor's
//! configured combo first and walks a short ladder of progressively
//! less-likely-to-conflict fallbacks, registering the first that takes.
//!
//! Building that ladder is pure list logic; registering it is not.

/// Fallbacks tried, in order, when the configured combo will not register.
///
/// Chosen to be unlikely to collide with OS or common-application
/// shortcuts. Order matters: the first entry is tried first.
pub const QUICK_HIDE_FALLBACKS: [&str; 3] = ["Alt+Shift+H", "Ctrl+Shift+J", "Ctrl+Alt+Shift+H"];

/// The combos to attempt, in order, for a configured quick-hide shortcut.
///
/// The configured combo always leads. Fallbacks follow, minus any that
/// duplicates it — trying the same combo twice would only produce a second
/// identical failure, and the log line that goes with it says "fallback",
/// which would be a lie.
#[must_use]
pub fn quick_hide_candidates(configured: &str) -> Vec<String> {
    let mut candidates = vec![configured.to_owned()];
    for fallback in QUICK_HIDE_FALLBACKS {
        if fallback != configured {
            candidates.push(fallback.to_owned());
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::{QUICK_HIDE_FALLBACKS, quick_hide_candidates};

    #[test]
    fn test_quick_hide_candidates_configured_combo_is_tried_first() {
        let candidates = quick_hide_candidates("Ctrl+Shift+Q");
        assert_eq!(candidates.first().map(String::as_str), Some("Ctrl+Shift+Q"));
        assert_eq!(candidates.len(), 1 + QUICK_HIDE_FALLBACKS.len());
    }

    #[test]
    fn test_quick_hide_candidates_preserves_fallback_order() {
        let candidates = quick_hide_candidates("Ctrl+Shift+Q");
        assert_eq!(candidates[1..], QUICK_HIDE_FALLBACKS.map(str::to_owned)[..]);
    }

    #[test]
    fn test_quick_hide_candidates_configured_equal_to_fallback_is_not_repeated() {
        let candidates = quick_hide_candidates("Ctrl+Shift+J");
        assert_eq!(
            candidates,
            vec!["Ctrl+Shift+J", "Alt+Shift+H", "Ctrl+Alt+Shift+H"]
        );
    }

    #[test]
    fn test_quick_hide_candidates_empty_configured_still_yields_fallbacks() {
        // An empty string parses to no shortcut and is skipped at
        // registration; the ladder below it must still be attempted.
        let candidates = quick_hide_candidates("");
        assert_eq!(candidates.len(), 1 + QUICK_HIDE_FALLBACKS.len());
        assert_eq!(candidates[1..], QUICK_HIDE_FALLBACKS.map(str::to_owned)[..]);
    }
}
