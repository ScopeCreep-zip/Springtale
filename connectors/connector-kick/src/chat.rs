//! Chat ingestion for Kick.
//!
//! Kick has no polling gateway: `chat.message.sent` arrives as a signed
//! webhook, verified by [`crate::webhook::verify_webhook`] and handed to
//! [`crate::KickConnector::dispatch_raw_webhook`]. That dispatch is this
//! source's inbound stream — it pushes each verified chat payload into
//! the bridge below, and [`ChatSource::run`] drains the bridge until
//! shutdown.
//!
//! Only the bot path is produced here. The rule-engine path for Kick
//! already exists: the webhook ingress emits the `ConnectorEvent`
//! itself, so these messages carry no `rule_events` — emitting them
//! again would fire every Kick recipe twice.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc, watch};

use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;

use crate::client::{KickApi, KickClient};

/// Connector name stamped on every emitted [`ChatMessage`].
pub const CONNECTOR_NAME: &str = "connector-kick";

/// The connector trigger whose payloads are chat.
pub const CHAT_TRIGGER: &str = "chat_message";

/// Depth of the webhook → chat bridge. Kick chat bursts; a bounded
/// queue drops the overflow loudly rather than growing without limit.
const BRIDGE_CAPACITY: usize = 256;

/// The Kick connector's inbound/outbound chat half.
pub struct KickChatSource {
    /// Outbound half — the connector's authenticated REST client,
    /// shared so a reply neither rebuilds the client nor copies the
    /// access token.
    client: Arc<KickClient>,
    /// Inbound bridge sender, fed by the connector's webhook dispatch.
    inbound_tx: mpsc::Sender<serde_json::Value>,
    /// Receiver, held behind a lock so only one `run` drains it.
    inbound_rx: Mutex<mpsc::Receiver<serde_json::Value>>,
}

impl KickChatSource {
    pub fn new(client: Arc<KickClient>) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(BRIDGE_CAPACITY);
        Self {
            client,
            inbound_tx,
            inbound_rx: Mutex::new(inbound_rx),
        }
    }

    /// Feed one dispatched webhook payload into the chat bridge.
    ///
    /// A no-op for every trigger but `chat_message`. Never blocks the
    /// webhook dispatch: a full bridge drops the message with a warning.
    pub fn ingest(&self, trigger: &str, payload: &serde_json::Value) {
        if trigger != CHAT_TRIGGER {
            return;
        }
        if let Err(e) = self.inbound_tx.try_send(payload.clone()) {
            tracing::warn!(error = %e, "Kick chat bridge full — dropping message");
        }
    }
}

/// Read an id field that Kick sends as a number or a string.
fn id_field(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Read a verified Kick `chat.message.sent` payload into a
/// [`ChatMessage`].
///
/// Shared by the two ways a Kick chat payload reaches the bot: this
/// source's webhook bridge, and the management API's webhook ingress
/// (via [`crate::webhook::ingest_event`]) — one extraction, one set of
/// field names.
///
/// `None` when the payload names neither a broadcaster nor a sender,
/// i.e. it is not a chat message at all.
#[must_use]
pub fn chat_message_from_payload(payload: &serde_json::Value) -> Option<ChatMessage> {
    // `channel_id` is the broadcaster's user id — the value Kick's
    // POST /public/v1/chat expects back.
    let channel_id = id_field(payload.get("broadcaster").and_then(|b| b.get("user_id")));
    let user_id = id_field(payload.get("sender").and_then(|s| s.get("user_id")));
    if channel_id.is_empty() && user_id.is_empty() {
        return None;
    }
    let text = payload
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_owned();
    Some(ChatMessage::chat(
        CONNECTOR_NAME,
        channel_id,
        user_id,
        text,
        payload.clone(),
    ))
}

#[async_trait]
impl ChatSource for KickChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        let mut inbound = self.inbound_rx.lock().await;
        tracing::info!("Kick chat source listening on the webhook bridge");

        loop {
            tokio::select! {
                next = inbound.recv() => {
                    let Some(payload) = next else {
                        tracing::info!("Kick chat bridge closed");
                        break;
                    };
                    let Some(msg) = chat_message_from_payload(&payload) else {
                        tracing::debug!("Kick payload carried no chat message — skipping");
                        continue;
                    };
                    if let Err(e) = tx.send(msg).await {
                        tracing::error!(error = %e, "failed to forward Kick chat message");
                        break;
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("Kick chat source received shutdown signal");
                        break;
                    }
                }
            }
        }

        tracing::info!("Kick chat source stopped");
        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        if channel_id.is_empty() {
            return Err(ConnectorError::ExecutionFailed(
                "kick reply needs a channel id".to_owned(),
            ));
        }
        self.client.send_chat(channel_id, text).await?;
        Ok(())
    }
}
