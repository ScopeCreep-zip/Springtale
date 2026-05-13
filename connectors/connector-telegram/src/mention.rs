//! Telegram mention extractor — pulls chat metadata from event
//! payloads into [`HarvestedDestination`]s for the universal
//! harvester.
//!
//! Telegram's `getUpdates` endpoint (already used by
//! `polling/mod.rs`) returns updates with a chat object nested
//! variably depending on the update kind:
//!
//! | Trigger                    | Chat path                  |
//! |---------------------------|----------------------------|
//! | `message_received`        | `chat`                     |
//! | `command_received`        | `chat`                     |
//! | `callback_query_received` | `message.chat`             |
//!
//! All three carry `id`, `type` (`"private" | "group" | "supergroup" | "channel"`),
//! and optionally `title` / `username` / `first_name`. We build a
//! `telegram://chat/{id}` workspace key and surface
//! `title || username || first_name || "Chat {id}"` as the
//! display name.
//!
//! ## Privacy
//!
//! The harvest writes only the chat title / username / first_name
//! to the directory's `display_name` column — never message
//! bodies, never user-message text. Per the sizes-only invariant.

use serde_json::Value;
use springtale_connector::mention::{HarvestedDestination, MentionExtractor};
use springtale_connector::workspace_key;

/// Telegram's [`MentionExtractor`] implementation.
pub struct TelegramMentionExtractor;

pub static TELEGRAM_MENTION_EXTRACTOR: TelegramMentionExtractor = TelegramMentionExtractor;

impl MentionExtractor for TelegramMentionExtractor {
    fn extract(&self, trigger: &str, payload: &Value) -> Vec<HarvestedDestination> {
        // Where does the chat object live in this payload?
        let chat = match trigger {
            "message_received" | "command_received" => payload.get("chat"),
            "callback_query_received" => payload.get("message").and_then(|m| m.get("chat")),
            _ => None,
        };
        let Some(chat) = chat else {
            return Vec::new();
        };
        let Some(harvested) = harvest_chat(chat) else {
            return Vec::new();
        };
        vec![harvested]
    }
}

/// Translate a Telegram `chat` JSON object into a
/// [`HarvestedDestination`]. Returns `None` if `chat.id` is
/// missing or non-numeric — those updates can't be addressed.
fn harvest_chat(chat: &Value) -> Option<HarvestedDestination> {
    let chat_id = chat.get("id").and_then(|v| v.as_i64())?;
    let chat_type = chat
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("private");
    let title = chat.get("title").and_then(|v| v.as_str());
    let username = chat.get("username").and_then(|v| v.as_str());
    let first_name = chat.get("first_name").and_then(|v| v.as_str());

    // Display name precedence: title (groups/channels) → username
    // (private chats with a username) → first_name (private chats
    // without a username) → numeric fallback.
    let display_name = title
        .map(str::to_owned)
        .or_else(|| username.map(|u| format!("@{u}")))
        .or_else(|| first_name.map(str::to_owned))
        .unwrap_or_else(|| format!("Chat {chat_id}"));

    let key = workspace_key::build("telegram", &["chat", &chat_id.to_string()]);

    // Metadata: only sizes-allowed extras (username for round-trip
    // display, chat type for the icon). Never the user's text.
    let mut metadata = serde_json::Map::new();
    if let Some(u) = username {
        metadata.insert("username".into(), Value::String(u.to_owned()));
    }
    metadata.insert("telegram_type".into(), Value::String(chat_type.to_owned()));

    Some(HarvestedDestination {
        workspace_key: key,
        display_name,
        kind: telegram_kind_to_workspace_kind(chat_type),
        metadata: Value::Object(metadata),
    })
}

/// Map Telegram's chat.type strings to the cooperation-layer
/// workspace `kind` taxonomy. Keeps the dropdown's icon /
/// filter logic uniform across connectors.
fn telegram_kind_to_workspace_kind(chat_type: &str) -> String {
    match chat_type {
        "private" => "user".to_owned(),
        "group" => "group".to_owned(),
        "supergroup" => "supergroup".to_owned(),
        "channel" => "channel".to_owned(),
        other => other.to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_from_message_received_with_private_chat() {
        let ext = TelegramMentionExtractor;
        let payload = json!({
            "message_id": 1,
            "chat": {
                "id": 12345,
                "type": "private",
                "first_name": "Alice",
                "username": "alicebsky",
            },
            "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
            "text": "hello",
            "date": 1700000000,
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "telegram://chat/12345");
        assert_eq!(out[0].kind, "user");
        // Title is absent → username takes precedence over first_name.
        assert_eq!(out[0].display_name, "@alicebsky");
    }

    #[test]
    fn extract_from_command_received_with_group() {
        let ext = TelegramMentionExtractor;
        let payload = json!({
            "message_id": 5,
            "chat": {
                "id": -1001000,
                "type": "supergroup",
                "title": "Sacramento Weather Group",
            },
            "command": "start",
            "date": 1700000000,
        });
        let out = ext.extract("command_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "telegram://chat/-1001000");
        assert_eq!(out[0].display_name, "Sacramento Weather Group");
        assert_eq!(out[0].kind, "supergroup");
        assert_eq!(out[0].metadata["telegram_type"], "supergroup");
    }

    #[test]
    fn extract_from_callback_query_unwraps_message_chat() {
        let ext = TelegramMentionExtractor;
        let payload = json!({
            "id": "callback123",
            "message": {
                "message_id": 7,
                "chat": {
                    "id": 9999,
                    "type": "private",
                    "first_name": "Bob",
                },
            },
            "data": "click",
        });
        let out = ext.extract("callback_query_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].workspace_key, "telegram://chat/9999");
        assert_eq!(out[0].display_name, "Bob");
    }

    #[test]
    fn extract_falls_back_to_numeric_when_no_label() {
        let ext = TelegramMentionExtractor;
        let payload = json!({
            "message_id": 1,
            "chat": { "id": 42, "type": "private" },
            "date": 1700000000,
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].display_name, "Chat 42");
    }

    #[test]
    fn extract_returns_empty_when_chat_id_missing() {
        let ext = TelegramMentionExtractor;
        let payload = json!({ "message_id": 1, "chat": { "type": "private" } });
        let out = ext.extract("message_received", &payload);
        assert!(out.is_empty());
    }

    #[test]
    fn extract_returns_empty_for_unknown_trigger() {
        let ext = TelegramMentionExtractor;
        let payload = json!({ "chat": { "id": 1, "type": "private" } });
        let out = ext.extract("some_unknown_trigger", &payload);
        assert!(out.is_empty());
    }

    #[test]
    fn channel_username_resolves_with_at_prefix() {
        let ext = TelegramMentionExtractor;
        let payload = json!({
            "chat": {
                "id": -2002000,
                "type": "channel",
                "username": "weatherbroadcast",
            },
            "message_id": 1,
            "date": 1,
        });
        let out = ext.extract("message_received", &payload);
        assert_eq!(out[0].display_name, "@weatherbroadcast");
        assert_eq!(out[0].kind, "channel");
    }
}
