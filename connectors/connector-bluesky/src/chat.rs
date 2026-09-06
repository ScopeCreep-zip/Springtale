//! Chat ingestion for Bluesky.
//!
//! This is the Jetstream firehose subscription the daemon used to own
//! (`wire_bluesky`), moved into the crate that owns the protocol.
//!
//! Bluesky firehose events are automation triggers, not interactive
//! chat: every message is built with
//! [`ChatMessage::rule_only`][springtale_connector::chat::ChatMessage::rule_only]
//! so it reaches the rule engine (`own_post`, `mention` recipes) and
//! never the bot's chat path.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;

use crate::client::{AtProtoClient, BlueskyApi};

/// Connector name stamped on every emitted [`ChatMessage`].
const CONNECTOR_NAME: &str = "connector-bluesky";

/// The Bluesky connector's firehose ingestion half.
pub struct BlueskyChatSource {
    /// The connector's authenticated client — shared, so resolving our
    /// own DID reuses the existing session instead of re-authenticating
    /// with the account password.
    client: Arc<AtProtoClient>,
    /// Jetstream WebSocket URL from config.
    jetstream_url: String,
}

impl BlueskyChatSource {
    pub fn new(client: Arc<AtProtoClient>, jetstream_url: String) -> Self {
        Self {
            client,
            jetstream_url,
        }
    }
}

#[async_trait]
impl ChatSource for BlueskyChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        // Our own DID classifies a firehose post as own_post (author ==
        // us) vs mention (a facet#mention referencing us).
        let (own_did, handle) = self.client.current_account().await?;
        tracing::info!(handle = %handle, did = %own_did, "Bluesky Jetstream gateway client ready");

        let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
            Arc::new(move |payload: serde_json::Value| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    // rule_only: firehose events are automation
                    // triggers, never interactive chat. The event name
                    // comes from the payload's own `"trigger"` field,
                    // the classification the gateway already made.
                    let msg =
                        ChatMessage::rule_only(CONNECTOR_NAME, payload).with_classified_event();
                    if let Err(e) = tx.send(msg).await {
                        tracing::error!(error = %e, "failed to forward Bluesky firehose event");
                    }
                });
            });

        crate::gateway::gateway_loop(self.jetstream_url.clone(), own_did, dispatcher, shutdown)
            .await;

        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        // Bluesky has no per-channel chat surface: the account's feed is
        // the only "channel", and a threaded reply needs the parent
        // post's uri *and* cid, which a channel id alone cannot carry.
        // An empty channel (what rule_only messages carry) posts to our
        // own feed; anything else is refused rather than silently
        // published to the wrong place.
        if !channel_id.is_empty() {
            return Err(ConnectorError::ExecutionFailed(format!(
                "connector-bluesky has no per-channel send: a threaded reply needs the parent \
                 uri and cid — use the `reply` action (channel: {channel_id})"
            )));
        }
        self.client.create_post(text).await?;
        Ok(())
    }
}
