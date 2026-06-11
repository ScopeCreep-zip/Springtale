//! Discord mention extractor — pulls (guild_id, channel_id) tuples
//! from event payloads into the formation's external-workspace
//! directory.
//!
//! Discord triggers all carry `channel_id` directly and (for
//! guild-scoped events) `guild_id`. DMs have a `channel_id` but no
//! `guild_id`. We build:
//!
//! ```text
//! discord://guild/{guild_id}/channel/{channel_id}
//! discord://dm/{channel_id}
//! ```
//!
//! ## Privacy
//!
//! `display_name` carries the channel id only when no human label
//! is available — guild + channel name resolution requires REST
//! lookups which the active `discover_destinations` action does.
//! Passive harvest is sizes-only.

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

pub struct DiscordMentionExtractor;

/// Single static instance — `DiscordMentionExtractor` is
/// zero-sized and stateless, so a static reference is the right
/// shape for the connector's `mention_extractor()` return.
pub static DISCORD_MENTION_EXTRACTOR: DiscordMentionExtractor = DiscordMentionExtractor;

impl MentionExtractor for DiscordMentionExtractor {
    fn extract(&self, _trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        // All Discord triggers that mention a chat carry `channel_id`
        // directly at the top level. Generic across slash_command_received,
        // message_received, mention_received, reaction_added.
        let channel_id = payload
            .get("channel_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let Some(channel_id) = channel_id else {
            return Vec::new();
        };
        let guild_id = payload
            .get("guild_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let (key, kind, display_name) = match guild_id.as_deref() {
            Some(gid) => (
                workspace_key::build("discord", &["guild", gid, "channel", &channel_id]),
                "channel".to_owned(),
                format!("#{channel_id}"),
            ),
            None => (
                workspace_key::build("discord", &["dm", &channel_id]),
                "dm".to_owned(),
                format!("DM {channel_id}"),
            ),
        };

        let mut metadata = serde_json::Map::new();
        if let Some(g) = guild_id {
            metadata.insert("guild_id".into(), Value::String(g));
        }
        metadata.insert("channel_id".into(), Value::String(channel_id));

        vec![HarvestedDestination {
            workspace_key: key,
            display_name,
            kind,
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
    fn extract_guild_channel() {
        let ext = DiscordMentionExtractor;
        let payload = json!({
            "message_id": "M1",
            "channel_id": "C123",
            "guild_id": "G456",
            "user_id": "U7",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "discord://guild/G456/channel/C123");
        assert_eq!(out[0].kind, "channel");
        assert_eq!(out[0].metadata["guild_id"], "G456");
        assert_eq!(out[0].metadata["channel_id"], "C123");
    }

    #[test]
    fn extract_dm_without_guild() {
        let ext = DiscordMentionExtractor;
        let payload = json!({
            "message_id": "M1",
            "channel_id": "C999",
            "user_id": "U7",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "discord://dm/C999");
        assert_eq!(out[0].kind, "dm");
    }

    #[test]
    fn extract_empty_when_no_channel_id() {
        let ext = DiscordMentionExtractor;
        let payload = json!({ "user_id": "U7" });
        assert!(ext.extract("message_received", &payload).is_empty());
    }
}
