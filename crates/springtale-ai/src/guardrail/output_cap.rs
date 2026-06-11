//! Output-size cap helper.
//!
//! A runaway provider should not be able to pipe arbitrarily large
//! bodies into the next step of a chain — `${last_ai_output}` template
//! substitution multiplies the cost downstream (memory + context
//! window in any follow-up AI step).
//!
//! Cap chosen at 64 KiB to match the connector WASM memory cap and to
//! be generous enough for any plausible chat turn (a typical English
//! response of 16 K tokens is ~64 KB of UTF-8).

/// Default output cap. Matches the connector WASM memory page count
/// (64 MiB / 1024 pages = 64 KiB per page) for symmetry with the
/// rest of the platform's resource bounds.
pub const DEFAULT_OUTPUT_CAP_BYTES: usize = 64 * 1024;

/// Truncate `content` so it fits within `cap_bytes`. Returns
/// `(maybe_truncated, was_truncated)`. Truncation cuts on a UTF-8
/// codepoint boundary — never produces invalid UTF-8.
///
/// We measure byte length (`len()`) rather than character count
/// because the cap exists to bound BYTES going onto the wire and
/// into downstream substitution buffers, not to bound a notional
/// "visible character count".
pub(crate) fn truncate_to_cap(content: String, cap_bytes: usize) -> (String, bool) {
    if content.len() <= cap_bytes {
        return (content, false);
    }
    let mut end = cap_bytes;
    // Walk back to a char boundary — `String::truncate` would panic
    // mid-codepoint otherwise.
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = content;
    out.truncate(end);
    out.push_str("\n…[truncated by springtale-ai guardrail]");
    (out, true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn under_cap_passes_through() {
        let s = "hello".to_string();
        let (out, truncated) = truncate_to_cap(s.clone(), 100);
        assert!(!truncated);
        assert_eq!(out, s);
    }

    #[test]
    fn over_cap_truncates() {
        let s = "x".repeat(200);
        let (out, truncated) = truncate_to_cap(s, 50);
        assert!(truncated);
        // 50-byte body + suffix
        assert!(out.starts_with(&"x".repeat(50)));
        assert!(out.contains("truncated"));
    }

    #[test]
    fn respects_char_boundary() {
        // Multi-byte glyph straddling the cap. Cap at a byte inside
        // the glyph — truncation must back up to the previous boundary.
        let s = "aaa\u{1F600}bbb".to_string(); // 'aaa' + 4-byte emoji + 'bbb' = 10 bytes
        let (out, truncated) = truncate_to_cap(s, 5); // cap mid-emoji
        assert!(truncated);
        // Must still be valid UTF-8.
        assert!(out.is_char_boundary(out.find('\n').unwrap_or(out.len())));
        assert!(out.starts_with("aaa"));
    }
}
