//! IRC mention extractor — pulls (channel | nick) from PRIVMSG /
//! command events.
//!
//! IRC events carry `nick` (the message sender) and `target` (the
//! channel name or the bot's nick for DMs). We disambiguate by
//! the `#` / `&` prefix on `target`:
//!
//! - Starts with `#` or `&` → channel.
//!   `irc://network/{network}/channel/{name}`
//! - Otherwise (DM to bot, `target == bot_nick`) → user.
//!   `irc://network/{network}/user/{nick}` (use sender's nick).
//!
//! Network name is sourced from the payload's `network` field
//! when set; otherwise defaults to `"default"`.

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

pub struct IrcMentionExtractor;

pub static IRC_MENTION_EXTRACTOR: IrcMentionExtractor = IrcMentionExtractor;

impl MentionExtractor for IrcMentionExtractor {
    fn extract(&self, _trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        let network = payload
            .get("network")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_owned();
        let target = payload
            .get("target")
            .or_else(|| payload.get("channel"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let nick = payload
            .get("nick")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let (id, kind, key) = match target.as_deref() {
            Some(t) if t.starts_with('#') || t.starts_with('&') => {
                let key = workspace_key::build("irc", &["network", &network, "channel", t]);
                (t.to_owned(), "channel".to_owned(), key)
            }
            _ => {
                // DM — the addressable party is the sender, not the
                // target (which was the bot itself).
                let Some(nick) = nick else {
                    return Vec::new();
                };
                let key = workspace_key::build("irc", &["network", &network, "user", &nick]);
                (nick.clone(), "user".to_owned(), key)
            }
        };

        let mut metadata = serde_json::Map::new();
        metadata.insert("network".into(), Value::String(network));

        vec![HarvestedDestination {
            workspace_key: key,
            display_name: id,
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
    fn extract_channel_with_hash_prefix() {
        let ext = IrcMentionExtractor;
        let payload = json!({
            "nick": "alice",
            "target": "#dev",
            "message": "hi",
            "network": "libera",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "irc://network/libera/channel/#dev");
        assert_eq!(out[0].kind, "channel");
        assert_eq!(out[0].display_name, "#dev");
    }

    #[test]
    fn extract_channel_with_ampersand_prefix() {
        let ext = IrcMentionExtractor;
        let payload = json!({
            "nick": "alice",
            "target": "&local",
            "message": "hi",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out[0].workspace_key, "irc://network/default/channel/&local");
        assert_eq!(out[0].kind, "channel");
    }

    #[test]
    fn extract_dm_uses_sender_nick() {
        let ext = IrcMentionExtractor;
        let payload = json!({
            "nick": "alice",
            "target": "bot_nick",
            "message": "hi",
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "irc://network/default/user/alice");
        assert_eq!(out[0].kind, "user");
        assert_eq!(out[0].display_name, "alice");
    }

    #[test]
    fn extract_user_joined_uses_channel_field() {
        let ext = IrcMentionExtractor;
        let payload = json!({
            "nick": "alice",
            "channel": "#general",
        });
        let out = ext.extract("user_joined", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].workspace_key,
            "irc://network/default/channel/#general"
        );
    }

    #[test]
    fn extract_empty_when_dm_has_no_nick() {
        let ext = IrcMentionExtractor;
        let payload = json!({ "target": "bot_nick" });
        assert!(ext.extract("message_received", &payload).is_empty());
    }
}
