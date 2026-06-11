//! URL grammar extractor.
//!
//! Pulls the first http(s) URL out of a message so a "watch this page"
//! recipe can pre-fill its `Url` input from the same sentence.

/// Extract the first http/https URL in `text`, if any. Splits on
/// whitespace (URLs don't contain spaces) and trims trailing
/// sentence punctuation.
pub fn parse_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let trimmed = word.trim_end_matches(['.', ',', ')', '!', '?', ';']);
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
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
    fn test_extracts_https() {
        assert_eq!(
            parse_url("watch https://example.com/news for changes").unwrap(),
            "https://example.com/news"
        );
    }

    #[test]
    fn test_trims_trailing_punctuation() {
        assert_eq!(
            parse_url("scrape https://example.com.").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn test_no_url() {
        assert!(parse_url("scrape my favorite site").is_none());
    }
}
