//! Slack chat ingestion — the Socket Mode loop the daemon used to own.
//!
//! Before [`springtale_connector::chat::ChatSource`] existed, this loop
//! lived in `apps/springtaled/src/runtime/connectors/slack.rs` and was
//! built from a typed TOML config, so a Slack connector installed at
//! runtime could never receive chat. The protocol belongs to the crate
//! that speaks it; the runtime only starts and stops it.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretBox;
use tokio::sync::{mpsc, watch};

use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;

use crate::client::{SlackApi, SlackClient};
use crate::config::SlackConfig;
use crate::error::SlackError;

/// Registry name this source reports on every [`ChatMessage`].
pub const CONNECTOR_NAME: &str = "connector-slack";

/// Slack's inbound/outbound half: Socket Mode WebSocket in,
/// `chat.postMessage` out.
///
/// Socket Mode needs no public HTTP endpoint — it works behind firewalls,
/// NAT and VPNs, which is why it is the only inbound mode here.
pub struct SlackChatSource {
    /// App-level token (`xapp-…`) for the Socket Mode WebSocket. Stays
    /// wrapped; only cloned out at the connection call site.
    app_token: SecretBox<String>,
    /// Shared with the connector so replies reuse one API client (and
    /// one copy of the bot token).
    client: Arc<SlackClient>,
}

impl SlackChatSource {
    /// Build the source from the connector's config and its client.
    ///
    /// Token formats are validated by `SlackConnector::new` before this
    /// runs, so nothing here can fail on a config the connector accepted.
    pub fn new(config: &SlackConfig, client: Arc<SlackClient>) -> Result<Self, SlackError> {
        // SECURITY: expose needed to clone the app token into this
        // source's own SecretBox — the original stays zeroize-on-drop.
        let app_token = SecretBox::new(Box::new(
            secrecy::ExposeSecret::expose_secret(&config.app_token).clone(),
        ));

        Ok(Self { app_token, client })
    }

    /// Build the gateway dispatcher: routed Slack payload → [`ChatMessage`].
    ///
    /// No rule-engine events are attached: Slack payloads reach the rule
    /// engine through the connector's own trigger subscriptions, not
    /// through this stream.
    fn build_dispatcher(
        tx: mpsc::Sender<ChatMessage>,
    ) -> Arc<dyn Fn(serde_json::Value) + Send + Sync> {
        Arc::new(move |payload: serde_json::Value| {
            let tx = tx.clone();
            let raw = payload.clone();
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
                // For slash commands, use the command + text; for messages, use text
                let text = if let Some(command) = payload.get("command").and_then(|c| c.as_str()) {
                    let args = payload.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    if args.is_empty() {
                        command.to_owned()
                    } else {
                        format!("{command} {args}")
                    }
                } else {
                    payload
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned()
                };

                let chat_msg = ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, raw);
                if let Err(e) = tx.send(chat_msg).await {
                    tracing::error!(error = %e, "failed to send Slack message to bot");
                }
            });
        })
    }
}

#[async_trait]
impl ChatSource for SlackChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        // SECURITY: expose needed for Socket Mode WebSocket connection
        let app_token = secrecy::ExposeSecret::expose_secret(&self.app_token).clone();
        let dispatcher = Self::build_dispatcher(tx);

        tracing::info!("Slack Socket Mode gateway started");
        crate::gateway::gateway_loop(app_token, dispatcher, shutdown).await;
        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        self.client
            .send_message(channel_id, text)
            .await
            .map(|_| ())
            .map_err(ConnectorError::from)
    }
}
