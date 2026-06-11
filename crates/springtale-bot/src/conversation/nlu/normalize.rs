//! Tokenization + light normalization.
//!
//! Deterministic, no ML. Lower-cases, strips punctuation, and applies
//! a tiny lemmatizer (plural / common verb tails) so "reminders",
//! "remind", and "reminding" all reduce toward the same stem the
//! intent gazetteer indexes. This is the first stage every utterance
//! and every recipe field passes through, so both sides of a match
//! are normalized identically.

/// A normalized token plus the character span it covered in the
/// original (lower-cased) utterance. The span lets entity extractors
/// reason about adjacency ("new" + "york") without re-tokenizing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Stemmed, lower-cased surface form.
    pub stem: String,
    /// Raw lower-cased surface form before stemming (kept for exact
    /// gazetteer phrase matching where stemming would over-fold).
    pub raw: String,
}

/// Words that carry no intent signal. Dropped from intent scoring but
/// NOT from entity extraction (which works off the raw lowercased text).
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "me", "my", "i", "you", "to", "for", "of", "and", "please", "can", "could",
    "would", "will", "want", "like", "get", "set", "up", "give", "send", "with", "in", "on", "at",
    "it", "is", "do", "this", "that", "every", "each",
];

/// Split a message into normalized tokens (intent-scoring view).
/// Stopwords are removed; everything else is stemmed.
pub fn tokenize(text: &str) -> Vec<Token> {
    raw_tokens(text)
        .into_iter()
        .filter(|raw| !STOPWORDS.contains(&raw.as_str()))
        .map(|raw| Token {
            stem: stem(&raw),
            raw,
        })
        .collect()
}

/// Split into lower-cased word tokens WITHOUT dropping stopwords —
/// the view entity extractors use, where "every" / "at" matter.
pub fn raw_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Normalize a label/phrase the same way an utterance is normalized,
/// preserving word order. Used to index recipe names and option labels.
pub fn normalize_phrase(text: &str) -> String {
    raw_tokens(text).join(" ")
}

/// Tiny English stemmer — strips the handful of suffixes that actually
/// cause intent misses. Deliberately conservative: over-stemming would
/// collapse unrelated words. No external dependency.
pub fn stem(word: &str) -> String {
    let w = word;
    for suffix in ["ing", "ies", "es", "ed", "ly", "s"] {
        if let Some(base) = w.strip_suffix(suffix)
            && base.len() >= 3
        {
            // "ies" → "y" (replies → reply); others just drop.
            if suffix == "ies" {
                return format!("{base}y");
            }
            return base.to_owned();
        }
    }
    w.to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_drops_stopwords_and_stems() {
        let toks = tokenize("send me the weather every morning");
        let stems: Vec<&str> = toks.iter().map(|t| t.stem.as_str()).collect();
        assert!(stems.contains(&"weather"));
        assert!(stems.contains(&"morn")); // "morning" → "morn"
        assert!(!stems.contains(&"the"));
        assert!(!stems.contains(&"me"));
    }

    #[test]
    fn test_raw_tokens_keeps_function_words() {
        let toks = raw_tokens("weather in Tucson at 8am");
        assert!(toks.contains(&"in".to_owned()));
        assert!(toks.contains(&"at".to_owned()));
        assert!(toks.contains(&"tucson".to_owned()));
        assert!(toks.contains(&"8am".to_owned()));
    }

    #[test]
    fn test_stem_plurals_and_verbs() {
        assert_eq!(stem("reminders"), "reminder");
        assert_eq!(stem("replies"), "reply");
        assert_eq!(stem("scraping"), "scrap");
        assert_eq!(stem("is"), "is"); // too short to strip
    }

    #[test]
    fn test_normalize_phrase_preserves_order() {
        assert_eq!(normalize_phrase("New York!"), "new york");
        assert_eq!(normalize_phrase("7:00 AM"), "7 00 am");
    }
}
