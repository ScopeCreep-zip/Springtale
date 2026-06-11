//! Number grammar extractor.
//!
//! Reads digits and small spelled-out words ("three", "twice") so a
//! recipe input of kind `Number` can be pre-filled from a sentence.

use crate::conversation::nlu::normalize::raw_tokens;

const WORDS: &[(&str, i64)] = &[
    ("zero", 0),
    ("one", 1),
    ("once", 1),
    ("two", 2),
    ("twice", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("twelve", 12),
    ("twenty", 20),
];

/// Extract the first standalone number in `text` (digit or small word).
pub fn parse_number(text: &str) -> Option<i64> {
    for tok in raw_tokens(text) {
        if let Ok(n) = tok.parse::<i64>() {
            return Some(n);
        }
        if let Some((_, n)) = WORDS.iter().find(|(w, _)| *w == tok) {
            return Some(*n);
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_digits() {
        assert_eq!(parse_number("keep the last 5 items").unwrap(), 5);
    }

    #[test]
    fn test_words() {
        assert_eq!(parse_number("remind me twice").unwrap(), 2);
        assert_eq!(parse_number("three times").unwrap(), 3);
    }

    #[test]
    fn test_none() {
        assert!(parse_number("no count here").is_none());
    }
}
