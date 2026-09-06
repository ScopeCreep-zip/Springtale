//! Reading a verified Telegram webhook `Update` into platform types.
//!
//! This extraction used to live in the daemon (`api/webhooks.rs`), keyed
//! off a `match` on the connector name — which is why no other
//! connector's webhook chat could reach the bot. It belongs to the crate
//! that speaks the protocol.

use serde_json::Value;

use springtale_connector::chat::ChatMessage;
use springtale_connector::webhook::WebhookIngest;

use crate::chat::CONNECTOR_NAME;

/// Read one verified Telegram `Update` into the chat messages it means.
///
/// Mirrors the polling dispatcher's field extraction
/// ([`crate::chat::TelegramChatSource`]) so webhook-mode and
/// polling-mode chat reach the bot identically.
///
/// No rule events are attached: the webhook ingress dispatches the
/// route's own `ConnectorEvent`, so returning it again would fire every
/// matching recipe twice.
#[must_use]
pub fn ingest_update(payload: &Value) -> WebhookIngest {
    if let Some(message) = payload.get("message") {
        return match message_fields(message) {
            Some((channel_id, user_id, text)) => WebhookIngest::message(ChatMessage::chat(
                CONNECTOR_NAME,
                channel_id,
                user_id,
                text,
                payload.clone(),
            )),
            None => WebhookIngest::empty(),
        };
    }

    // Inline keyboard button press: the callback data is the text, so
    // handlers treat it as a command-like input.
    if let Some(callback) = payload.get("callback_query") {
        return match callback_fields(callback) {
            Some((channel_id, user_id, text)) => WebhookIngest::message(ChatMessage::chat(
                CONNECTOR_NAME,
                channel_id,
                user_id,
                text,
                payload.clone(),
            )),
            None => WebhookIngest::empty(),
        };
    }

    WebhookIngest::empty()
}

/// `(channel_id, user_id, text)` from a Telegram `message` object.
fn message_fields(message: &Value) -> Option<(String, String, String)> {
    let user_id = numeric_id(message.get("from")?.get("id"))?;
    let channel_id = numeric_id(message.get("chat")?.get("id"))?;
    let text = message
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Some((channel_id, user_id, text))
}

/// `(channel_id, user_id, text)` from a Telegram `callback_query`.
fn callback_fields(callback: &Value) -> Option<(String, String, String)> {
    let user_id = numeric_id(callback.get("from")?.get("id"))?;
    let channel_id = numeric_id(callback.get("message")?.get("chat")?.get("id"))?;
    let text = callback
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Some((channel_id, user_id, text))
}

/// Telegram ids are 64-bit integers; render one as the string every
/// downstream consumer uses.
fn numeric_id(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_i64).map(|i| i.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_update_message_returns_chat_message() {
        let update = serde_json::json!({
            "message": {
                "from": { "id": 42 },
                "chat": { "id": -100 },
                "text": "/help"
            }
        });
        let ingest = ingest_update(&update);
        assert_eq!(ingest.messages.len(), 1);
        let msg = &ingest.messages[0];
        assert_eq!(msg.connector, CONNECTOR_NAME);
        assert_eq!(msg.user_id, "42");
        assert_eq!(msg.channel_id, "-100");
        assert_eq!(msg.text, "/help");
        assert!(ingest.events.is_empty());
    }

    #[test]
    fn test_ingest_update_callback_query_returns_chat_message() {
        let update = serde_json::json!({
            "callback_query": {
                "id": "cb1",
                "from": { "id": 7 },
                "message": { "chat": { "id": 9 } },
                "data": "confirm"
            }
        });
        let ingest = ingest_update(&update);
        assert_eq!(ingest.messages.len(), 1);
        assert_eq!(ingest.messages[0].text, "confirm");
        assert_eq!(ingest.messages[0].channel_id, "9");
    }

    #[test]
    fn test_ingest_update_unknown_shape_returns_empty() {
        let update = serde_json::json!({ "edited_channel_post": { "text": "x" } });
        assert!(ingest_update(&update).is_empty());
    }
}
