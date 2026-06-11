//! Workspace-key URI parsing — the shared format every messaging
//! connector and the cooperation layer agree on for addressing
//! external destinations.
//!
//! ## URI shape
//!
//! ```text
//! <scheme>://<segment1>/<segment2>/...
//! ```
//!
//! Per-connector conventions (each connector owns its scheme,
//! mapped 1:1 to the connector's name minus the `connector-`
//! prefix):
//!
//! ```text
//! telegram://chat/{chat_id}
//! telegram://channel/{username}
//! discord://guild/{guild_id}/channel/{channel_id}
//! discord://dm/{channel_id}
//! slack://channel/{channel_id}
//! slack://im/{user_id}
//! signal://group/{group_id}
//! signal://user/{phone_number}
//! irc://network/{network}/channel/{channel_name}
//! irc://network/{network}/user/{nick}
//! nostr://pubkey/{pubkey_hex}
//! bluesky://account/{did}
//! ```
//!
//! ## Design rationale
//!
//! - The cooperation layer's `WorkspaceKey(String)` type
//!   (`springtale-cooperation::types::WorkspaceKey`) treats the
//!   URI opaquely. This module is the boundary parser.
//! - Each connector's `send_message` action accepts EITHER a raw
//!   destination id (`"12345"`) OR a full URI string
//!   (`"telegram://chat/12345"`). [`extract_id_for_scheme`] does
//!   the at-boundary translation so connectors stay
//!   backwards-compatible with all hand-written recipe TOML.
//! - No external URI-parsing crate. The format is constrained
//!   enough that a tiny parser does it correctly without dragging
//!   in `url`'s 200kB of WHATWG-compliance code we don't need.

/// Result of parsing a `<scheme>://<segments...>` string. Holds
/// borrowed slices so this is allocation-free in the common path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedWorkspaceKey<'a> {
    /// The connector's URI scheme (e.g. `"telegram"`, `"discord"`).
    /// Matches the connector name with the `connector-` prefix
    /// stripped — see [`scheme_for_connector`].
    pub scheme: &'a str,
    /// Path segments after `://`, split on `/`. `["chat", "12345"]`
    /// for `telegram://chat/12345`. Empty segments (e.g. trailing
    /// slashes) are dropped.
    pub segments: Vec<&'a str>,
}

impl<'a> ParsedWorkspaceKey<'a> {
    /// The last segment — usually the "id" portion of the URI.
    /// Returns `None` when there are no segments.
    pub fn last(&self) -> Option<&'a str> {
        self.segments.last().copied()
    }

    /// Segment at a specific index. Returns `None` past the end.
    pub fn segment(&self, idx: usize) -> Option<&'a str> {
        self.segments.get(idx).copied()
    }
}

/// Parse a URI-shaped workspace key. Returns `None` when the
/// input doesn't contain `://` or the scheme/path parts are
/// empty.
///
/// Whitespace-only inputs and inputs missing the scheme delimiter
/// are not URIs — callers should treat them as raw ids.
pub fn parse(input: &str) -> Option<ParsedWorkspaceKey<'_>> {
    let trimmed = input.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    if scheme.is_empty() {
        return None;
    }
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }
    Some(ParsedWorkspaceKey { scheme, segments })
}

/// Build a URI string from a scheme + segments. The inverse of
/// [`parse`]. Empty segments are skipped (don't put `/` in the
/// middle of an id).
pub fn build(scheme: &str, segments: &[&str]) -> String {
    let mut s = String::with_capacity(
        scheme.len() + 3 + segments.iter().map(|seg| seg.len() + 1).sum::<usize>(),
    );
    s.push_str(scheme);
    s.push_str("://");
    let mut first = true;
    for seg in segments.iter().filter(|s| !s.is_empty()) {
        if !first {
            s.push('/');
        }
        s.push_str(seg);
        first = false;
    }
    s
}

/// Strip the `connector-` prefix from a connector name to get its
/// URI scheme. `connector-telegram` → `telegram`,
/// `connector-discord` → `discord`. Inputs without the prefix are
/// returned unchanged (defensive — keeps community connector
/// naming flexible).
pub fn scheme_for_connector(connector_name: &str) -> &str {
    connector_name
        .strip_prefix("connector-")
        .unwrap_or(connector_name)
}

/// At-boundary translator for connector send-actions. Given a
/// user-supplied input string (which is either a raw id or a URI)
/// and the connector's name, returns the raw id to hand off to
/// the remote API.
///
/// Rules:
/// - If `input` parses as a URI AND its scheme matches the
///   connector's scheme, return the last segment as the raw id.
/// - If `input` parses as a URI but the scheme doesn't match,
///   return `Err(ParseError::WrongScheme)` — the user pasted a
///   destination from a different connector by mistake.
/// - If `input` doesn't parse as a URI, treat it as a raw id and
///   return it as-is (backwards-compatible with hand-written TOML).
pub fn extract_id_for_scheme<'a>(
    input: &'a str,
    connector_name: &str,
) -> Result<&'a str, ParseError<'a>> {
    let trimmed = input.trim();
    match parse(trimmed) {
        Some(parsed) => {
            let want = scheme_for_connector(connector_name);
            if parsed.scheme == want {
                parsed.last().ok_or(ParseError::Empty)
            } else {
                Err(ParseError::WrongScheme {
                    got: parsed.scheme,
                    want: want.to_owned(),
                })
            }
        }
        None => {
            if trimmed.is_empty() {
                Err(ParseError::Empty)
            } else {
                // Raw id (no `://`). Backwards-compatible path.
                Ok(trimmed)
            }
        }
    }
}

/// Failure modes for [`extract_id_for_scheme`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError<'a> {
    #[error("workspace key is empty")]
    Empty,
    #[error(
        "workspace key scheme mismatch: got `{got}`, expected `{want}` \
         (you may have pasted a destination from a different connector)"
    )]
    WrongScheme { got: &'a str, want: String },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_two_segment() {
        let p = parse("telegram://chat/12345").unwrap();
        assert_eq!(p.scheme, "telegram");
        assert_eq!(p.segments, vec!["chat", "12345"]);
        assert_eq!(p.last(), Some("12345"));
    }

    #[test]
    fn parse_nested_path() {
        let p = parse("discord://guild/G123/channel/C456").unwrap();
        assert_eq!(p.scheme, "discord");
        assert_eq!(p.segments, vec!["guild", "G123", "channel", "C456"]);
        assert_eq!(p.last(), Some("C456"));
        assert_eq!(p.segment(1), Some("G123"));
    }

    #[test]
    fn parse_trims_whitespace() {
        let p = parse("  slack://channel/C1  ").unwrap();
        assert_eq!(p.scheme, "slack");
        assert_eq!(p.last(), Some("C1"));
    }

    #[test]
    fn parse_drops_trailing_slash() {
        let p = parse("telegram://chat/12345/").unwrap();
        assert_eq!(p.segments, vec!["chat", "12345"]);
    }

    #[test]
    fn parse_returns_none_for_raw_id() {
        assert!(parse("12345").is_none());
        assert!(parse("@channelname").is_none());
    }

    #[test]
    fn parse_returns_none_for_empty_path() {
        assert!(parse("telegram://").is_none());
        assert!(parse("telegram:///").is_none());
    }

    #[test]
    fn parse_returns_none_for_empty_scheme() {
        assert!(parse("://chat/12345").is_none());
    }

    #[test]
    fn build_round_trips() {
        let s = build("telegram", &["chat", "12345"]);
        assert_eq!(s, "telegram://chat/12345");
        let p = parse(&s).unwrap();
        assert_eq!(p.scheme, "telegram");
        assert_eq!(p.segments, vec!["chat", "12345"]);
    }

    #[test]
    fn build_handles_empty_segments() {
        let s = build("telegram", &["chat", "", "12345"]);
        assert_eq!(s, "telegram://chat/12345");
    }

    #[test]
    fn scheme_for_connector_strips_prefix() {
        assert_eq!(scheme_for_connector("connector-telegram"), "telegram");
        assert_eq!(scheme_for_connector("connector-discord"), "discord");
    }

    #[test]
    fn scheme_for_connector_passes_through_unprefixed() {
        assert_eq!(scheme_for_connector("custom-bot"), "custom-bot");
    }

    #[test]
    fn extract_id_returns_last_segment_for_matching_uri() {
        let id = extract_id_for_scheme("telegram://chat/12345", "connector-telegram").unwrap();
        assert_eq!(id, "12345");
    }

    #[test]
    fn extract_id_returns_last_segment_for_nested_uri() {
        let id = extract_id_for_scheme("discord://guild/G/channel/C", "connector-discord").unwrap();
        assert_eq!(id, "C");
    }

    #[test]
    fn extract_id_falls_back_to_raw_id_when_no_uri() {
        let id = extract_id_for_scheme("12345", "connector-telegram").unwrap();
        assert_eq!(id, "12345");
    }

    #[test]
    fn extract_id_falls_back_for_username_style() {
        let id = extract_id_for_scheme("@channelname", "connector-telegram").unwrap();
        assert_eq!(id, "@channelname");
    }

    #[test]
    fn extract_id_rejects_wrong_scheme() {
        let err = extract_id_for_scheme("discord://channel/C1", "connector-telegram").unwrap_err();
        assert!(matches!(err, ParseError::WrongScheme { .. }));
    }

    #[test]
    fn extract_id_rejects_empty_input() {
        let err = extract_id_for_scheme("", "connector-telegram").unwrap_err();
        assert!(matches!(err, ParseError::Empty));
        let err = extract_id_for_scheme("   ", "connector-telegram").unwrap_err();
        assert!(matches!(err, ParseError::Empty));
    }
}
