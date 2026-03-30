//! Shared encoding utilities for connectors.
//!
//! Provides percent-encoding and base64url encoding so that individual
//! connector crates do not duplicate these helpers.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Percent-encode a string for use in URL parameters.
///
/// Unreserved characters (RFC 3986 §2.3) are passed through unchanged.
/// All other characters are percent-encoded.
///
/// When `space_as_plus` is `true`, spaces are encoded as `+` (application/
/// x-www-form-urlencoded style). When `false`, spaces are encoded as `%20`
/// (RFC 3986 style).
pub fn urlencoded(s: &str, space_as_plus: bool) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => {
                if space_as_plus {
                    result.push('+');
                } else {
                    result.push_str("%20");
                }
            }
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push('%');
                    result.push_str(&format!("{byte:02X}"));
                }
            }
        }
    }
    result
}

/// Base64url encoding without padding (RFC 4648 section 5).
///
/// Uses the `URL_SAFE_NO_PAD` engine from the `base64` crate.
pub fn base64url_encode(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoded_space_as_plus() {
        assert_eq!(urlencoded("hello world", true), "hello+world");
    }

    #[test]
    fn test_urlencoded_space_as_percent() {
        assert_eq!(urlencoded("hello world", false), "hello%20world");
    }

    #[test]
    fn test_urlencoded_special_chars() {
        assert_eq!(urlencoded("rust+lang", true), "rust%2Blang");
        assert_eq!(urlencoded("rust+lang", false), "rust%2Blang");
    }

    #[test]
    fn test_urlencoded_unreserved_passthrough() {
        assert_eq!(urlencoded("simple", true), "simple");
        assert_eq!(urlencoded("a-b_c.d~e", false), "a-b_c.d~e");
    }

    #[test]
    fn test_base64url_encode_no_padding() {
        let encoded = base64url_encode(b"Hello");
        assert_eq!(encoded, "SGVsbG8");
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
    }

    #[test]
    fn test_base64url_encode_empty() {
        assert_eq!(base64url_encode(b""), "");
    }
}
