use regex::Regex;

/// A compiled pattern entry for keyword/regex matching.
pub struct PatternEntry {
    /// The command name this pattern maps to.
    pub command: String,
    /// Compiled regex pattern.
    pub regex: Regex,
}

/// Pattern-based router for non-slash triggers.
pub struct PatternRouter {
    patterns: Vec<PatternEntry>,
}

impl PatternRouter {
    pub fn new(patterns: Vec<PatternEntry>) -> Self {
        Self { patterns }
    }

    /// Returns `Some((command_name, full_text))` if text matches a pattern.
    pub fn try_match(&self, text: &str) -> Option<(String, String)> {
        for entry in &self.patterns {
            if entry.regex.is_match(text) {
                return Some((entry.command.clone(), text.to_owned()));
            }
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_match() {
        let patterns = vec![PatternEntry {
            command: "greet".into(),
            regex: Regex::new(r"(?i)^(hello|hi|hey)\b").unwrap(),
        }];
        let router = PatternRouter::new(patterns);

        let result = router.try_match("hello there");
        assert!(result.is_some());
        let (cmd, _) = result.unwrap();
        assert_eq!(cmd, "greet");
    }

    #[test]
    fn test_pattern_no_match() {
        let patterns = vec![PatternEntry {
            command: "greet".into(),
            regex: Regex::new(r"(?i)^(hello|hi|hey)\b").unwrap(),
        }];
        let router = PatternRouter::new(patterns);
        assert!(router.try_match("goodbye").is_none());
    }

    #[test]
    fn test_pattern_empty() {
        let router = PatternRouter::new(vec![]);
        assert!(router.try_match("anything").is_none());
    }

    #[test]
    fn test_pattern_first_match_wins() {
        let patterns = vec![
            PatternEntry {
                command: "first".into(),
                regex: Regex::new(r"test").unwrap(),
            },
            PatternEntry {
                command: "second".into(),
                regex: Regex::new(r"test").unwrap(),
            },
        ];
        let router = PatternRouter::new(patterns);
        let (cmd, _) = router.try_match("test input").unwrap();
        assert_eq!(cmd, "first");
    }
}
