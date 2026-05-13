//! Slack mention extractor — pulls channel_id from event payloads
//! and classifies the kind by the Slack ID prefix.
//!
//! Slack's id prefixes are stable:
//!
//! | Prefix | Kind                       |
//! |--------|----------------------------|
//! | `C`    | public_channel             |
//! | `G`    | private_channel (legacy)   |
//! | `D`    | IM (direct message)        |
//! | `M`    | mpim (multi-party IM)      |
//!
//! Generates `slack://channel/{C…}` for channels and
//! `slack://im/{D…}` for direct messages.

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

pub struct SlackMentionExtractor;

pub static SLACK_MENTION_EXTRACTOR: SlackMentionExtractor = SlackMentionExtractor;

impl MentionExtractor for SlackMentionExtractor {
    fn extract(&self, _trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        // Most Slack triggers carry `channel_id`; `reaction_added`
        // uses `item_channel`. Check both.
        let channel = payload
            .get("channel_id")
            .or_else(|| payload.get("item_channel"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let Some(channel) = channel else {
            return Vec::new();
        };
        if channel.is_empty() {
            return Vec::new();
        }
        let kind = match channel.chars().next() {
            Some('C') => "channel",
            Some('G') => "private_channel",
            Some('D') => "dm",
            Some('M') => "group",
            _ => "channel",
        };
        let (segment, key) = match kind {
            "dm" => ("im", workspace_key::build("slack", &["im", &channel])),
            _ => ("channel", workspace_key::build("slack", &["channel", &channel])),
        };
        let _ = segment;

        let mut metadata = serde_json::Map::new();
        metadata.insert("channel_id".into(), Value::String(channel.clone()));

        vec![HarvestedDestination {
            workspace_key: key,
            display_name: format!("#{channel}"),
            kind: kind.to_owned(),
            metadata: Value::Object(metadata),
        }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_public_channel() {
        let ext = SlackMentionExtractor;
        let payload = json!({ "channel_id": "C12345", "user_id": "U1", "ts": "1.0" });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "slack://channel/C12345");
        assert_eq!(out[0].kind, "channel");
    }

    #[test]
    fn extract_dm_channel() {
        let ext = SlackMentionExtractor;
        let payload = json!({ "channel_id": "D99", "user_id": "U1", "ts": "1.0" });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "slack://im/D99");
        assert_eq!(out[0].kind, "dm");
    }

    #[test]
    fn extract_private_channel_prefix_g() {
        let ext = SlackMentionExtractor;
        let payload = json!({ "channel_id": "GXYZ" });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out[0].kind, "private_channel");
    }

    #[test]
    fn extract_handles_reaction_added_item_channel() {
        let ext = SlackMentionExtractor;
        let payload = json!({ "user_id": "U1", "reaction": "thumbsup", "item_channel": "C42" });
        let out = ext.extract("reaction_added", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "slack://channel/C42");
    }

    #[test]
    fn extract_empty_when_no_channel() {
        let ext = SlackMentionExtractor;
        let payload = json!({ "user_id": "U1" });
        assert!(ext.extract("message_received", &payload).is_empty());
    }
}
