//! Concept → keyword expansion (a tiny hand-built thesaurus).
//!
//! Recipe authors name things one way ("Morning Weather"); users say
//! them another ("forecast", "temperature", "is it going to rain").
//! This static table bridges the gap deterministically: when an
//! utterance stem matches a concept's trigger, the concept's canonical
//! keywords are added to the intent-scoring bag so the right recipe
//! still wins. No ML, no I/O — just a curated list that grows as new
//! recipe families land.

/// One concept: any `triggers` stem in the utterance contributes all
/// `expands_to` stems to the scoring bag.
pub struct Concept {
    pub triggers: &'static [&'static str],
    pub expands_to: &'static [&'static str],
}

/// The thesaurus. Triggers/expansions are already stemmed (so they
/// match `normalize::stem` output) — keep them short.
pub const CONCEPTS: &[Concept] = &[
    Concept {
        triggers: &[
            "forecast",
            "temperature",
            "temp",
            "rain",
            "rainy",
            "sunny",
            "cloud",
            "degree",
        ],
        expands_to: &["weather"],
    },
    Concept {
        triggers: &["alarm", "remind", "reminder", "nudge", "alert", "ping"],
        expands_to: &["reminder", "remind"],
    },
    Concept {
        triggers: &["text", "message", "dm", "chat", "notify", "tell"],
        expands_to: &["message", "send"],
    },
    Concept {
        triggers: &["scrape", "crawl", "watch", "monitor", "track", "extract"],
        expands_to: &["scrape", "browser", "web"],
    },
    Concept {
        triggers: &[
            "summary",
            "summarize",
            "digest",
            "recap",
            "briefing",
            "brief",
        ],
        expands_to: &["summary", "digest"],
    },
    Concept {
        triggers: &["repost", "crosspost", "mirror", "relay", "syndicate"],
        expands_to: &["relay", "crosspost"],
    },
    Concept {
        triggers: &["journal", "diary", "reflect", "gratitude"],
        expands_to: &["journal"],
    },
];

/// Return the canonical expansion stems triggered by a single utterance
/// stem (empty if the stem triggers no concept).
pub fn expand(stem: &str) -> &'static [&'static str] {
    for c in CONCEPTS {
        if c.triggers.contains(&stem) {
            return c.expands_to;
        }
    }
    &[]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_forecast_expands_to_weather() {
        assert!(expand("forecast").contains(&"weather"));
        assert!(expand("rain").contains(&"weather"));
    }

    #[test]
    fn test_unknown_stem_expands_to_nothing() {
        assert!(expand("zxcv").is_empty());
    }
}
