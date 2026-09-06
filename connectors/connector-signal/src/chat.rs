//! Signal chat ingestion — the SSE receive loop this connector owns.
//!
//! Ported from the daemon's `wire_signal`: the loop against the
//! signal-cli daemon's `/api/v1/events` stream now lives with the
//! connector that speaks the protocol. The runtime only starts and
//! stops it.

use std::sync::Arc;

use async_trait::async_trait;
use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;
use tokio::sync::{mpsc, watch};

use crate::actions;
use crate::client::SignalClient;
use crate::config::SignalConfig;

/// Registry name this source reports on every [`ChatMessage`].
pub const CONNECTOR_NAME: &str = "connector-signal";

/// Signal's [`ChatSource`] — SSE inbound, signal-cli JSON-RPC outbound.
///
/// The signal-cli daemon must be started separately by the user:
/// `signal-cli -a +NUMBER daemon --http localhost:PORT`.
pub struct SignalChatSource {
    /// Shared with the connector so replies reuse one HTTP pool and
    /// one jitter configuration.
    client: Arc<SignalClient>,
    daemon_url: String,
}

impl SignalChatSource {
    /// Build the source from the connector's config. No credentials
    /// are held here — signal-cli owns all Signal Protocol material.
    pub fn new(config: &SignalConfig, client: Arc<SignalClient>) -> Self {
        Self {
            client,
            daemon_url: config.daemon_url.clone(),
        }
    }
}

#[async_trait]
impl ChatSource for SignalChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        // Dispatcher: signal-cli envelopes → ChatMessage. The rule-engine
        // event the gateway classified travels on the message itself
        // (`with_classified_event` reads the payload's "trigger" field)
        // rather than through a separate emit.
        let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
            Arc::new(move |payload: serde_json::Value| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let user_id = payload
                        .get("user_id")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let channel_id = payload
                        .get("channel_id")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let text = payload
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned();

                    let message =
                        ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, payload)
                            .with_classified_event();
                    if let Err(e) = tx.send(message).await {
                        tracing::error!(error = %e, "failed to forward Signal message");
                    }
                });
            });

        tracing::info!("Signal gateway starting (SSE listener)");
        crate::gateway::gateway_loop(self.daemon_url.clone(), dispatcher, shutdown).await;
        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        // Same path as the `send_message` action, including its
        // WorkspaceKey-URI parsing of the recipient.
        let input = serde_json::json!({ "chat_id": channel_id, "text": text });
        actions::send_message::execute(self.client.as_ref(), &input)
            .await
            .map_err(ConnectorError::from)?;
        Ok(())
    }
}
