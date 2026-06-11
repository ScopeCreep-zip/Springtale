//! Free-text place extraction (no ML).
//!
//! Pulls a place phrase out of a sentence so a universal recipe with a
//! geocoded target (a city) can be filled in one shot — "weather **for
//! Sacramento, CA** every morning" → "Sacramento, CA". The geocoder is
//! the gazetteer (it resolves the phrase at deploy time), so here we
//! only need the cue-word grammar to grab the candidate phrase. Returns
//! the ORIGINAL-case substring (not lowercased tokens) so the geocoder
//! and the confirmation see "Sacramento, CA".

/// Cue words that introduce a place ("weather **in** Tucson", "change
/// **to** London").
const CUES: &[&str] = &["in", "for", "near", "at", "around", "to"];

/// Change verbs that license "it" as a cue ("make **it** Tucson") without
/// letting "set it up" read "up" as a place.
const CHANGE_VERBS: &[&str] = &["make", "makes", "making", "change", "changes", "changed"];

/// Words that end a place phrase — times/frequencies and conjunctions
/// ("weather in Tucson **every** morning" stops the place at "Tucson").
const STOPPERS: &[&str] = &[
    "every",
    "each",
    "daily",
    "weekly",
    "morning",
    "afternoon",
    "evening",
    "night",
    "tonight",
    "noon",
    "midnight",
    "at",
    "every",
    "and",
    "then",
    "please",
    "tomorrow",
    "today",
];

/// Extract the first place phrase in `text`, if any. Looks for a cue
/// word, then takes the words after it up to a stopper / end, preserving
/// original case and the comma in "Sacramento, CA".
pub fn parse_place(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();

    for (i, w) in words.iter().enumerate() {
        let cue = w
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        let is_cue = CUES.contains(&cue.as_str())
            || (cue == "it"
                && i > 0
                && CHANGE_VERBS.contains(
                    &words[i - 1]
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                        .as_str(),
                ));
        if !is_cue {
            continue;
        }
        // "at" is also a time cue ("at 8am") — skip if the next token is a time.
        if cue == "at"
            && let Some(next) = words.get(i + 1)
            && super::time::parse_time(next).is_some()
        {
            continue;
        }

        let mut phrase: Vec<&str> = Vec::new();
        for w2 in &words[i + 1..] {
            let bare = w2
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if bare.is_empty() || STOPPERS.contains(&bare.as_str()) {
                break;
            }
            phrase.push(w2);
        }
        if phrase.is_empty() {
            continue;
        }
        // Join, then trim trailing sentence punctuation but keep internal
        // commas ("Sacramento, CA").
        let joined = phrase.join(" ");
        let trimmed = joined.trim_end_matches(['.', '!', '?', ';', ':']).trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_place_after_for_with_comma() {
        assert_eq!(
            parse_place("can you check the weather for Sacramento, CA every morning?").as_deref(),
            Some("Sacramento, CA")
        );
    }

    #[test]
    fn test_place_after_in() {
        assert_eq!(
            parse_place("weather in Tucson every morning").as_deref(),
            Some("Tucson")
        );
    }

    #[test]
    fn test_multiword_place() {
        assert_eq!(
            parse_place("set up the weather for New York City").as_deref(),
            Some("New York City")
        );
    }

    #[test]
    fn test_at_time_is_not_a_place() {
        // "at 8am" is a time, not a place.
        assert!(parse_place("weather at 8am").is_none());
        // but "at London" is a place
        assert_eq!(parse_place("weather at London").as_deref(), Some("London"));
    }

    #[test]
    fn test_no_cue_no_place() {
        assert!(parse_place("set up the morning weather").is_none());
    }

    #[test]
    fn test_make_it_correction() {
        assert_eq!(
            parse_place("actually make it Tucson").as_deref(),
            Some("Tucson")
        );
        assert_eq!(parse_place("change to London").as_deref(), Some("London"));
    }

    #[test]
    fn test_set_it_up_is_not_a_place() {
        // "set" is not a change-verb, so "it" isn't a cue → "up" isn't a place.
        assert!(parse_place("set it up").is_none());
        assert!(parse_place("yes set it up").is_none());
    }

    #[test]
    fn test_trailing_punctuation_trimmed() {
        assert_eq!(
            parse_place("weather for London.").as_deref(),
            Some("London")
        );
    }
}
