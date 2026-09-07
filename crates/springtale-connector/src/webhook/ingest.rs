//! What a verified webhook payload means, in the platform's own types.

use crate::chat::ChatMessage;

/// One rule-engine event a webhook payload carries, beyond the trigger
/// the ingress already dispatches for the request path itself.
///
/// A connector returns these only when a single verified request means
/// *more* than its path trigger — the ingress always dispatches the
/// path's own `ConnectorEvent`, so repeating it here would fire every
/// matching recipe twice.
#[derive(Debug, Clone)]
pub struct WebhookEvent {
    /// `ConnectorEvent` name, as declared in
    /// [`crate::connector::trait_::Connector::triggers`].
    pub event: String,
    /// Payload the recipe sees (normalized centrally, downstream).
    pub payload: serde_json::Value,
}

impl WebhookEvent {
    /// Build an event from its name and payload.
    pub fn new(event: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            event: event.into(),
            payload,
        }
    }
}

/// One action the ingress should execute back on the connector that
/// produced this ingest, to complete the request the platform sent.
///
/// Some chat platforms require the receiver to answer a specific inbound
/// event inside a timeout — an inline-button press has to be
/// acknowledged or the user's button spins until the platform gives up.
/// That answer is protocol knowledge, so the connector names it; the
/// ingress only executes it, through the same capability-checked
/// registry path any other action takes, without knowing what it is.
///
/// The daemon used to hold one connector's version of this as a literal
/// `if trigger == "..."` in the HTTP route, which is why exactly one
/// connector's webhooks could be acknowledged and no other's could.
#[derive(Debug, Clone)]
pub struct WebhookAck {
    /// Action name, as declared in
    /// [`crate::connector::trait_::Connector::actions`].
    pub action: String,
    /// Input for that action.
    pub input: serde_json::Value,
}

impl WebhookAck {
    /// Build an acknowledgement from an action name and its input.
    pub fn new(action: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            action: action.into(),
            input,
        }
    }
}

/// The result of reading a verified webhook payload.
///
/// Both halves reuse the platform's existing types: `messages` are the
/// same [`ChatMessage`] the polling gateways push through
/// [`crate::chat::ChatSource`], so webhook-mode chat and polling-mode
/// chat reach the bot down one path.
#[derive(Debug, Clone, Default)]
pub struct WebhookIngest {
    /// Chat messages the payload carries, bound for the bot runtime
    /// (subject to each message's `deliver_to_bot`).
    pub messages: Vec<ChatMessage>,
    /// Additional rule-engine events the payload carries.
    pub events: Vec<WebhookEvent>,
    /// Actions the ingress runs back on this connector to complete the
    /// request (see [`WebhookAck`]).
    pub acks: Vec<WebhookAck>,
}

impl WebhookIngest {
    /// Nothing to ingest — the default for connectors without webhooks
    /// and for payloads a connector does not recognize.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// One chat message, no extra rule events.
    #[must_use]
    pub fn message(msg: ChatMessage) -> Self {
        Self {
            messages: vec![msg],
            events: Vec::new(),
            acks: Vec::new(),
        }
    }

    /// Attach an acknowledgement the ingress should execute back on this
    /// connector (see [`WebhookAck`]).
    #[must_use]
    pub fn with_ack(mut self, ack: WebhookAck) -> Self {
        self.acks.push(ack);
        self
    }

    /// Whether this ingest carries nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.events.is_empty() && self.acks.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_ingest_carries_no_acks() {
        let ingest = WebhookIngest::empty();
        assert!(ingest.acks.is_empty());
        assert!(ingest.is_empty());
    }

    #[test]
    fn test_with_ack_records_action_and_input() {
        let ingest = WebhookIngest::empty().with_ack(WebhookAck::new(
            "ack_action",
            serde_json::json!({ "id": "x" }),
        ));
        assert_eq!(ingest.acks.len(), 1);
        assert_eq!(ingest.acks[0].action, "ack_action");
        assert_eq!(ingest.acks[0].input["id"], "x");
        // An ack alone is still something to do.
        assert!(!ingest.is_empty());
    }
}
