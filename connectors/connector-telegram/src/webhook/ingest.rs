//! Reading a verified Telegram webhook `Update` into platform types.
//!
//! This extraction used to live in the daemon (`api/webhooks.rs`), keyed
//! off a `match` on the connector name — which is why no other
//! connector's webhook chat could reach the bot. It belongs to the crate
//! that speaks the protocol.

use serde_json::Value;

use springtale_connector::chat::ChatMessage;
use springtale_connector::webhook::{WebhookAck, WebhookIngest};

use crate::chat::CONNECTOR_NAME;

/// Action that answers an inline-button press. Telegram times the press
/// out after ten seconds, after which the user's button spins forever.
const ANSWER_CALLBACK_QUERY: &str = "answer_callback_query";

/// Trigger the webhook route uses for an inline-button press.
const CALLBACK_TRIGGER: &str = "callback_query_received";

/// Read one verified Telegram `Update` into the chat messages it means,
/// plus the `answerCallbackQuery` an inline-button press owes the user.
///
/// Mirrors the polling dispatcher's field extraction and its immediate
/// acknowledgement ([`crate::chat::TelegramChatSource`]) so webhook-mode
/// and polling-mode chat behave identically. The acknowledgement used to
/// live in the daemon's HTTP route as a literal check on this trigger
/// name and this action name — the one connector the route knew.
///
/// No rule events are attached: the webhook ingress dispatches the
/// route's own `ConnectorEvent`, so returning it again would fire every
/// matching recipe twice.
#[must_use]
pub fn ingest_update(trigger: &str, payload: &Value) -> WebhookIngest {
    let ingest = if let Some(message) = payload.get("message") {
        match message_fields(message) {
            Some((channel_id, user_id, text)) => WebhookIngest::message(ChatMessage::chat(
                CONNECTOR_NAME,
                channel_id,
                user_id,
                text,
                payload.clone(),
            )),
            None => WebhookIngest::empty(),
        }
    } else if let Some(callback) = payload.get("callback_query") {
        // Inline keyboard button press: the callback data is the text, so
        // handlers treat it as a command-like input.
        match callback_fields(callback) {
            Some((channel_id, user_id, text)) => WebhookIngest::message(ChatMessage::chat(
                CONNECTOR_NAME,
                channel_id,
                user_id,
                text,
                payload.clone(),
            )),
            None => WebhookIngest::empty(),
        }
    } else {
        WebhookIngest::empty()
    };

    match callback_query_id(trigger, payload) {
        Some(id) => ingest.with_ack(WebhookAck::new(
            ANSWER_CALLBACK_QUERY,
            serde_json::json!({ "callback_query_id": id }),
        )),
        None => ingest,
    }
}

/// The `callback_query.id` that has to be answered, if this payload is a
/// button press.
///
/// Two shapes are accepted. A genuine Telegram webhook posts an `Update`,
/// so the id sits under `callback_query`. The daemon route this replaced
/// read a top-level `id` instead, which is the shape a caller posting a
/// bare `callback_query` object sends; that reading is kept, still gated
/// on the trigger the route gated it on, so nothing that worked before
/// stops working.
fn callback_query_id<'a>(trigger: &str, payload: &'a Value) -> Option<&'a str> {
    if let Some(id) = payload
        .get("callback_query")
        .and_then(|cb| cb.get("id"))
        .and_then(Value::as_str)
    {
        return Some(id);
    }
    if trigger == CALLBACK_TRIGGER {
        return payload.get("id").and_then(Value::as_str);
    }
    None
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
        let ingest = ingest_update("message_received", &update);
        assert_eq!(ingest.messages.len(), 1);
        let msg = &ingest.messages[0];
        assert_eq!(msg.connector, CONNECTOR_NAME);
        assert_eq!(msg.user_id, "42");
        assert_eq!(msg.channel_id, "-100");
        assert_eq!(msg.text, "/help");
        assert!(ingest.events.is_empty());
        assert!(ingest.acks.is_empty());
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
        let ingest = ingest_update("callback_query_received", &update);
        assert_eq!(ingest.messages.len(), 1);
        assert_eq!(ingest.messages[0].text, "confirm");
        assert_eq!(ingest.messages[0].channel_id, "9");
    }

    /// The acknowledgement the HTTP route used to hard-code now comes
    /// from the connector that owns the protocol.
    #[test]
    fn test_ingest_update_callback_query_asks_for_answer_callback_query() {
        let update = serde_json::json!({
            "callback_query": {
                "id": "cb1",
                "from": { "id": 7 },
                "message": { "chat": { "id": 9 } },
                "data": "confirm"
            }
        });
        let ingest = ingest_update("callback_query_received", &update);
        assert_eq!(ingest.acks.len(), 1);
        assert_eq!(ingest.acks[0].action, "answer_callback_query");
        assert_eq!(ingest.acks[0].input["callback_query_id"], "cb1");
    }

    /// The shape the old daemon route read: a bare callback_query object
    /// with the id at the top level, gated on the trigger name.
    #[test]
    fn test_ingest_update_bare_callback_payload_still_acknowledged() {
        let payload = serde_json::json!({ "id": "cb2", "data": "confirm" });
        let ingest = ingest_update("callback_query_received", &payload);
        assert_eq!(ingest.acks.len(), 1);
        assert_eq!(ingest.acks[0].input["callback_query_id"], "cb2");
        assert!(ingest.messages.is_empty());
    }

    /// A plain message carries a top-level `id` in some payload shapes;
    /// it must never be answered as a button press.
    #[test]
    fn test_ingest_update_message_trigger_never_acknowledges() {
        let payload = serde_json::json!({ "id": "not-a-callback" });
        assert!(ingest_update("message_received", &payload).acks.is_empty());
    }

    #[test]
    fn test_ingest_update_unknown_shape_returns_empty() {
        let update = serde_json::json!({ "edited_channel_post": { "text": "x" } });
        assert!(ingest_update("message_received", &update).is_empty());
    }
}
