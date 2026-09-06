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
        }
    }

    /// Whether this ingest carries nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.events.is_empty()
    }
}
