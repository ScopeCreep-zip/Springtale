//! Reading a verified Kick webhook payload into platform types.
//!
//! Kick has no polling gateway — `chat.message.sent` arrives only as a
//! signed webhook. Until the ingress could ask a connector what a
//! payload means, Kick chat had no route to the bot at all: the daemon
//! extracted fields for one hardcoded connector and dropped everything
//! else.

use serde_json::Value;

use springtale_connector::webhook::WebhookIngest;

use crate::chat::{CHAT_TRIGGER, chat_message_from_payload};

/// Header carrying the Kick event type (`Kick-Event-Type`).
pub const HEADER_EVENT_TYPE: &str = "kick-event-type";

/// Kick's event type for a chat message.
pub const CHAT_EVENT_TYPE: &str = "chat.message.sent";

/// Read one verified Kick webhook into the chat messages it means.
///
/// The route's trigger name is authoritative; the `Kick-Event-Type`
/// header is the fallback for a webhook registered under a Kick event
/// type rather than the connector's trigger name.
///
/// No rule events are attached: the webhook ingress dispatches the
/// route's own `ConnectorEvent`, so returning it again would fire every
/// Kick recipe twice.
#[must_use]
pub fn ingest_event(
    trigger: &str,
    headers: &std::collections::HashMap<String, String>,
    payload: &Value,
) -> WebhookIngest {
    if !is_chat(trigger, headers) {
        return WebhookIngest::empty();
    }
    match chat_message_from_payload(payload) {
        Some(msg) => WebhookIngest::message(msg),
        None => WebhookIngest::empty(),
    }
}

/// Whether this request carries a Kick chat message.
fn is_chat(trigger: &str, headers: &std::collections::HashMap<String, String>) -> bool {
    if trigger == CHAT_TRIGGER {
        return true;
    }
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(HEADER_EVENT_TYPE))
        .is_some_and(|(_, v)| v == CHAT_EVENT_TYPE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat_payload() -> Value {
        serde_json::json!({
            "message_id": "m1",
            "broadcaster": { "user_id": 555, "username": "streamer" },
            "sender": { "user_id": 777, "username": "viewer" },
            "content": "!uptime"
        })
    }

    #[test]
    fn test_ingest_event_chat_trigger_returns_chat_message() {
        let ingest = ingest_event(
            CHAT_TRIGGER,
            &std::collections::HashMap::new(),
            &chat_payload(),
        );
        assert_eq!(ingest.messages.len(), 1);
        let msg = &ingest.messages[0];
        assert_eq!(msg.connector, "connector-kick");
        assert_eq!(msg.channel_id, "555");
        assert_eq!(msg.user_id, "777");
        assert_eq!(msg.text, "!uptime");
        assert!(msg.deliver_to_bot);
        assert!(ingest.events.is_empty());
    }

    #[test]
    fn test_ingest_event_event_type_header_returns_chat_message() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Kick-Event-Type".to_owned(), CHAT_EVENT_TYPE.to_owned());
        let ingest = ingest_event("some_route", &headers, &chat_payload());
        assert_eq!(ingest.messages.len(), 1);
        assert_eq!(ingest.messages[0].text, "!uptime");
    }

    #[test]
    fn test_ingest_event_non_chat_trigger_returns_empty() {
        let ingest = ingest_event(
            "stream_live",
            &std::collections::HashMap::new(),
            &serde_json::json!({ "is_live": true }),
        );
        assert!(ingest.is_empty());
    }
}
