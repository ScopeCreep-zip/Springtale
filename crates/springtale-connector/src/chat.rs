//! Chat ingestion — the inbound/outbound split every chat connector owns.
//!
//! Precedent: Rasa splits a channel into an `InputChannel` (webhook →
//! `on_new_message`) and an `OutputChannel` (`send_text_message`); Hubot
//! adapters implement `run()` for inbound and `send()` for outbound.
//! [`ChatSource`] is that split.
//!
//! Before this existed, the daemon held one `wire_*` function per chat
//! connector, each building a receive loop from a typed TOML config —
//! so a connector installed at runtime could never receive chat. The
//! loop now lives in the connector crate that owns the protocol; the
//! runtime only starts and stops it, keyed off the registry.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use crate::error::ConnectorError;

/// One inbound message from a chat connector.
///
/// Moved here from `springtale_bot::IncomingMessage`: the bot is one
/// consumer of this stream, not its owner.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Connector name that produced the message (`connector-telegram`, …).
    pub connector: String,
    /// Channel / chat / room identifier, as the provider spells it.
    pub channel_id: String,
    /// Sender identifier, as the provider spells it.
    pub user_id: String,
    /// Message text (already unwrapped from the provider envelope).
    pub text: String,
    /// Raw provider payload — handlers that need provider-specific
    /// fields read them here rather than widening this struct.
    pub raw: serde_json::Value,
    /// `ConnectorEvent` names this payload classifies as, for the rule
    /// engine. Empty means "chat only, no rule event". A Telegram
    /// `/command` update classifies as both `message` and
    /// `command_received`, which is why this is a list.
    pub rule_events: Vec<String>,
    /// Whether the bot's chat path should see this. `false` for
    /// firehose-style sources (Bluesky Jetstream) whose events are
    /// automation triggers, not interactive chat.
    pub deliver_to_bot: bool,
}

impl ChatMessage {
    /// An interactive chat message bound for the bot.
    pub fn chat(
        connector: impl Into<String>,
        channel_id: impl Into<String>,
        user_id: impl Into<String>,
        text: impl Into<String>,
        raw: serde_json::Value,
    ) -> Self {
        Self {
            connector: connector.into(),
            channel_id: channel_id.into(),
            user_id: user_id.into(),
            text: text.into(),
            raw,
            rule_events: Vec::new(),
            deliver_to_bot: true,
        }
    }

    /// A rule-engine-only payload: no bot delivery.
    pub fn rule_only(connector: impl Into<String>, raw: serde_json::Value) -> Self {
        Self {
            connector: connector.into(),
            channel_id: String::new(),
            user_id: String::new(),
            text: String::new(),
            raw,
            rule_events: Vec::new(),
            deliver_to_bot: false,
        }
    }

    /// Attach the rule-engine event names this payload classifies as.
    #[must_use]
    pub fn with_events<I, S>(mut self, events: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rule_events = events.into_iter().map(Into::into).collect();
        self
    }

    /// Attach the single event name carried in the payload's own
    /// `"trigger"` field, when the gateway classified it that way.
    #[must_use]
    pub fn with_classified_event(self) -> Self {
        let event = self
            .raw
            .get("trigger")
            .and_then(|t| t.as_str())
            .map(std::borrow::ToOwned::to_owned);
        match event {
            Some(e) => self.with_events([e]),
            None => self,
        }
    }
}

/// A connector that can receive and send chat.
///
/// `run` owns the connector's receive loop — the same loop the daemon
/// used to build from a typed TOML config. `send` is the outbound half,
/// looked up by connector name when the bot replies.
#[async_trait]
pub trait ChatSource: Send + Sync {
    /// Run until `shutdown` flips to `true`; push every inbound message
    /// to `tx`. Returning `Ok(())` means a clean stop.
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError>;

    /// Send a reply to `channel_id`.
    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError>;
}

/// Convenience alias for the shared handle connectors hand out.
pub type SharedChatSource = Arc<dyn ChatSource>;
