use std::collections::HashSet;

/// Exact prefix router for `/command` style messages.
///
/// Supports both single-word commands (`/help`) and multi-word commands
/// (`/github create_issue`). Uses longest-match: if both `"github"` and
/// `"github create_issue"` are registered, `/github create_issue args`
/// matches the longer one.
#[derive(Default)]
pub struct PrefixRouter {
    commands: HashSet<String>,
}

impl PrefixRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command name (without the prefix character).
    /// Can be single-word ("help") or multi-word ("github create_issue").
    pub fn register(&mut self, command: &str) {
        self.commands.insert(command.to_lowercase());
    }

    /// Unregister a command name.
    pub fn unregister(&mut self, command: &str) {
        self.commands.remove(&command.to_lowercase());
    }

    /// Returns `Some((command_name, args))` if text starts with
    /// `{prefix_char}{command}`.
    ///
    /// Uses longest-match: tries the full text first, then progressively
    /// shorter prefixes. This correctly handles multi-word commands like
    /// `/github create_issue args` when "github create_issue" is registered.
    pub fn try_match(&self, text: &str, prefix_char: char) -> Option<(String, String)> {
        let text = text.trim();
        if !text.starts_with(prefix_char) {
            return None;
        }

        let without_prefix = &text[prefix_char.len_utf8()..];
        let lower = without_prefix.to_lowercase();

        // Try longest match first: check if the full remaining text
        // (minus trailing args) matches a registered command.
        // Split into words and try progressively shorter prefixes.
        let words: Vec<&str> = lower.split_whitespace().collect();
        for len in (1..=words.len()).rev() {
            let candidate = words[..len].join(" ");
            if self.commands.contains(&candidate) {
                // Args are everything after the matched words in the ORIGINAL text
                let matched_char_count: usize = without_prefix
                    .split_whitespace()
                    .take(len)
                    .map(|w| w.len())
                    .sum::<usize>()
                    + (len - 1); // spaces between words
                let args = without_prefix
                    .get(matched_char_count..)
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                return Some((candidate, args));
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
    fn test_prefix_match_simple() {
        let mut router = PrefixRouter::new();
        router.register("search");
        let result = router.try_match("/search", '/');
        assert_eq!(result, Some(("search".into(), String::new())));
    }

    #[test]
    fn test_prefix_match_with_args() {
        let mut router = PrefixRouter::new();
        router.register("search");
        let result = router.try_match("/search tokyo weather", '/');
        assert_eq!(result, Some(("search".into(), "tokyo weather".into())));
    }

    #[test]
    fn test_prefix_no_match() {
        let router = PrefixRouter::new();
        assert!(router.try_match("/unknown foo", '/').is_none());
    }

    #[test]
    fn test_prefix_case_insensitive() {
        let mut router = PrefixRouter::new();
        router.register("Help");
        let result = router.try_match("/HELP", '/');
        assert_eq!(result, Some(("help".into(), String::new())));
    }

    #[test]
    fn test_prefix_no_prefix_char() {
        let mut router = PrefixRouter::new();
        router.register("search");
        assert!(router.try_match("search foo", '/').is_none());
    }

    #[test]
    fn test_prefix_unregister() {
        let mut router = PrefixRouter::new();
        router.register("search");
        router.unregister("search");
        assert!(router.try_match("/search foo", '/').is_none());
    }

    #[test]
    fn test_prefix_whitespace_trimmed() {
        let mut router = PrefixRouter::new();
        router.register("help");
        let result = router.try_match("  /help  ", '/');
        assert_eq!(result, Some(("help".into(), String::new())));
    }

    #[test]
    fn test_prefix_multiword_command() {
        let mut router = PrefixRouter::new();
        router.register("github create_issue");
        let result = router.try_match("/github create_issue title goes here", '/');
        assert_eq!(
            result,
            Some(("github create_issue".into(), "title goes here".into()))
        );
    }

    #[test]
    fn test_prefix_multiword_no_args() {
        let mut router = PrefixRouter::new();
        router.register("github create_issue");
        let result = router.try_match("/github create_issue", '/');
        assert_eq!(result, Some(("github create_issue".into(), String::new())));
    }

    #[test]
    fn test_prefix_multiword_longest_match() {
        let mut router = PrefixRouter::new();
        router.register("github");
        router.register("github create_issue");
        // Longest match should win
        let result = router.try_match("/github create_issue my title", '/');
        assert_eq!(
            result,
            Some(("github create_issue".into(), "my title".into()))
        );
    }

    #[test]
    fn test_prefix_multiword_fallback_to_shorter() {
        let mut router = PrefixRouter::new();
        router.register("github");
        // "github list_repos" is NOT registered, should match "github"
        let result = router.try_match("/github list_repos", '/');
        assert_eq!(result, Some(("github".into(), "list_repos".into())));
    }

    #[test]
    fn test_prefix_multiword_case_insensitive() {
        let mut router = PrefixRouter::new();
        router.register("GitHub Create_Issue");
        let result = router.try_match("/github create_issue args", '/');
        assert_eq!(result, Some(("github create_issue".into(), "args".into())));
    }
}
