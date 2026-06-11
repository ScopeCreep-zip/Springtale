//! Boolean grammar extractor + affirmation/negation/cancel detection.
//!
//! Powers `Bool` slot fills ("turn quiet hours on") and the dialogue's
//! yes/no confirmation + cancel keywords, all deterministically.

use crate::conversation::nlu::normalize::raw_tokens;

const TRUE_WORDS: &[&str] = &["on", "yes", "yeah", "yep", "true", "enable", "enabled"];
const FALSE_WORDS: &[&str] = &["off", "no", "nope", "false", "disable", "disabled"];

/// Parse an explicit on/off answer. None when the text isn't boolean.
pub fn parse_bool(text: &str) -> Option<bool> {
    let toks = raw_tokens(text);
    if toks.iter().any(|t| TRUE_WORDS.contains(&t.as_str())) {
        return Some(true);
    }
    if toks.iter().any(|t| FALSE_WORDS.contains(&t.as_str())) {
        return Some(false);
    }
    None
}

/// Is this message an affirmation of a confirm prompt? Broader than
/// `parse_bool` — accepts "do it", "go", "sure", "sounds good".
pub fn is_affirmative(text: &str) -> bool {
    let toks = raw_tokens(text);
    const YES: &[&str] = &[
        "yes", "yeah", "yep", "yup", "sure", "ok", "okay", "go", "do", "deploy", "confirm",
        "create", "sounds", "good", "perfect", "please",
    ];
    toks.iter().any(|t| YES.contains(&t.as_str()))
}

/// Is this message a negation / "not yet"?
pub fn is_negative(text: &str) -> bool {
    let toks = raw_tokens(text);
    const NO: &[&str] = &["no", "nope", "wait", "not", "dont"];
    toks.iter().any(|t| NO.contains(&t.as_str()))
}

/// Is this message a request to abandon the current setup?
pub fn is_cancel(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("cancel")
        || lower.contains("never mind")
        || lower.contains("nevermind")
        || lower.contains("stop")
        || lower.contains("forget it")
        || lower.contains("quit")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("turn it on"), Some(true));
        assert_eq!(parse_bool("no thanks"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn test_affirmative_phrases() {
        assert!(is_affirmative("yes please"));
        assert!(is_affirmative("sounds good"));
        assert!(is_affirmative("go for it"));
        assert!(!is_affirmative("change the time"));
    }

    #[test]
    fn test_cancel() {
        assert!(is_cancel("never mind"));
        assert!(is_cancel("actually, cancel that"));
        assert!(!is_cancel("change the city"));
    }
}
