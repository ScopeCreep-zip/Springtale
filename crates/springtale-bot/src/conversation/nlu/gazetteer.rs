//! Gazetteer matching — find a known label inside an utterance.
//!
//! A gazetteer is a list of `(normalized_label, value)` pairs built
//! from a recipe `Select` field's options (e.g. "tucson" → the
//! Open-Meteo lat/long string). Matching is longest-phrase-first so
//! "new york" beats a stray "new", and falls back to transposition-
//! aware fuzzy matching for single-token typos ("tuscon" → "tucson").
//! This is the no-ML entity-resolution layer (Snips/Duckling-style).

use super::fuzzy;
use super::normalize::raw_tokens;

/// One entry: the label as the user might type it, and the option
/// `value` to store when it matches.
#[derive(Debug, Clone)]
pub struct GazEntry {
    /// Normalized label words (e.g. `["new", "york"]`).
    pub words: Vec<String>,
    /// The `SelectOption.value` emitted on a hit.
    pub value: String,
    /// The human label (for echoing back in NLG).
    pub label: String,
}

/// A per-slot gazetteer.
#[derive(Debug, Clone, Default)]
pub struct Gazetteer {
    pub entries: Vec<GazEntry>,
}

/// Result of matching a gazetteer against an utterance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GazHit {
    pub value: String,
    pub label: String,
    /// `true` when the hit required a fuzzy (typo-tolerant) match
    /// rather than an exact one — callers may want to confirm.
    pub fuzzy: bool,
}

impl Gazetteer {
    /// Build a gazetteer from `(value, label)` pairs.
    pub fn from_options(options: impl IntoIterator<Item = (String, String)>) -> Self {
        let entries = options
            .into_iter()
            .map(|(value, label)| GazEntry {
                words: raw_tokens(&label),
                value,
                label,
            })
            .collect();
        Self { entries }
    }

    /// Find the best label match inside `text`. Longest exact phrase
    /// wins; if no exact phrase matches, a single-token fuzzy match
    /// (≤1 transposition-aware edit on tokens ≥4 chars) is tried.
    pub fn match_in(&self, text: &str) -> Option<GazHit> {
        let toks = raw_tokens(text);

        // Pass 1: exact phrase, longest first.
        let mut best: Option<(usize, &GazEntry)> = None;
        for entry in &self.entries {
            if entry.words.is_empty() {
                continue;
            }
            if contains_subslice(&toks, &entry.words) {
                let len = entry.words.len();
                if best.is_none_or(|(blen, _)| len > blen) {
                    best = Some((len, entry));
                }
            }
        }
        if let Some((_, entry)) = best {
            return Some(GazHit {
                value: entry.value.clone(),
                label: entry.label.clone(),
                fuzzy: false,
            });
        }

        // Pass 2: single-token fuzzy (only for single-word labels —
        // multi-word labels are too risky to fuzzy-match token-wise).
        for entry in &self.entries {
            if entry.words.len() != 1 {
                continue;
            }
            let label_word = &entry.words[0];
            if label_word.len() < 4 {
                continue;
            }
            for t in &toks {
                if t.len() >= 4 && fuzzy::close_enough(t, label_word, 1) {
                    return Some(GazHit {
                        value: entry.value.clone(),
                        label: entry.label.clone(),
                        fuzzy: true,
                    });
                }
            }
        }
        None
    }
}

/// Does `haystack` contain `needle` as a contiguous run?
fn contains_subslice(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| {
        w.iter()
            .map(String::as_str)
            .eq(needle.iter().map(String::as_str))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn weather_locations() -> Gazetteer {
        Gazetteer::from_options([
            ("lat=phx".to_owned(), "Phoenix".to_owned()),
            ("lat=nyc".to_owned(), "New York".to_owned()),
            ("lat=tus".to_owned(), "Tucson".to_owned()),
        ])
    }

    #[test]
    fn test_exact_single_token() {
        let g = weather_locations();
        let hit = g.match_in("weather in tucson every morning").unwrap();
        assert_eq!(hit.value, "lat=tus");
        assert!(!hit.fuzzy);
    }

    #[test]
    fn test_multiword_phrase_wins() {
        let g = weather_locations();
        let hit = g.match_in("forecast for new york please").unwrap();
        assert_eq!(hit.value, "lat=nyc");
        assert_eq!(hit.label, "New York");
    }

    #[test]
    fn test_fuzzy_single_token_typo() {
        let g = weather_locations();
        let hit = g.match_in("weather in tuscon").unwrap();
        assert_eq!(hit.value, "lat=tus");
        assert!(hit.fuzzy);
    }

    #[test]
    fn test_no_match_returns_none() {
        let g = weather_locations();
        assert!(g.match_in("weather in london").is_none());
    }
}
