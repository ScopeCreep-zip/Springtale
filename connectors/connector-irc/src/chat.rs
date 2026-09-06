//! IRC chat ingestion — the reconnecting receive loop this connector owns.
//!
//! Ported from the daemon's `wire_irc`: the loop that drives
//! [`crate::gateway::gateway_loop`] now lives with the connector that
//! speaks the protocol. The runtime only starts and stops it.

use std::sync::Arc;

use async_trait::async_trait;
use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;
use tokio::sync::{mpsc, watch};

use crate::actions;
use crate::client::IrcClient;
use crate::config::IrcConfig;
use crate::error::IrcError;

/// Registry name this source reports on every [`ChatMessage`].
pub const CONNECTOR_NAME: &str = "connector-irc";

/// IRC's [`ChatSource`].
///
/// The receive loop opens its own connection (the `irc` crate's stream
/// half is not shareable with the connector's `Sender`), so inbound
/// traffic arrives on a second link while [`ChatSource::send`] goes out
/// over the connection the connector established. That split is
/// unchanged from the daemon wiring this replaces.
pub struct IrcChatSource {
    /// Shared with the connector so replies reuse its session state
    /// (joined channels, DM targets, publish jitter).
    client: Arc<IrcClient>,
    /// Connection parameters for the receive link. Holds the NickServ
    /// password as a bare `String` because the `irc` crate's `Config`
    /// has no `Secret<T>` in its public API — same exposure window as
    /// the connector's own connection, which holds an identical copy.
    gateway_config: irc::client::data::Config,
    command_prefix: String,
    sasl_enabled: bool,
}

impl IrcChatSource {
    /// Build the source from the connector's config.
    pub fn new(config: &IrcConfig, client: Arc<IrcClient>) -> Result<Self, IrcError> {
        Ok(Self {
            client,
            gateway_config: crate::connector::build_irc_config(config)?,
            command_prefix: config.command_prefix.clone(),
            sasl_enabled: config.sasl_enabled,
        })
    }
}

#[async_trait]
impl ChatSource for IrcChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        // Dispatcher: routed IRC payloads → ChatMessage. The rule-engine
        // event the gateway classified travels on the message itself
        // (`with_classified_event` reads the payload's "trigger" field)
        // rather than through a separate emit.
        let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
            Arc::new(move |payload: serde_json::Value| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let user_id = payload
                        .get("nick")
                        .or_else(|| payload.get("pubkey"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let channel_id = payload
                        .get("target")
                        .or_else(|| payload.get("channel"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let text = payload
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                        .to_owned();

                    let message =
                        ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, payload)
                            .with_classified_event();
                    if let Err(e) = tx.send(message).await {
                        tracing::error!(error = %e, "failed to forward IRC message");
                    }
                });
            });

        tracing::info!("IRC gateway starting");
        crate::gateway::gateway_loop(
            self.gateway_config.clone(),
            self.command_prefix.clone(),
            self.sasl_enabled,
            dispatcher,
            shutdown,
        )
        .await;
        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        // Same path as the `send_message` action, including its
        // WorkspaceKey-URI parsing of the target.
        let input = serde_json::json!({ "chat_id": channel_id, "text": text });
        actions::send_message::execute(self.client.as_ref(), &input)
            .await
            .map_err(ConnectorError::from)?;
        Ok(())
    }
}
