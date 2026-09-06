//! Discord chat ingestion — the gateway receive loop this connector owns.
//!
//! Ported from the daemon's `wire_discord`: intent construction, slash
//! command registration, shard creation and the gateway loop now live
//! with the connector that speaks the protocol. The runtime only starts
//! and stops it.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretBox;
use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;
use tokio::sync::{mpsc, watch};
use twilight_model::gateway::Intents;

use crate::actions;
use crate::client::{DiscordApi, DiscordClient};
use crate::config::{DiscordConfig, SlashCommandConfig};

/// Registry name this source reports on every [`ChatMessage`].
pub const CONNECTOR_NAME: &str = "connector-discord";

/// Discord's [`ChatSource`] — gateway inbound, REST outbound.
pub struct DiscordChatSource {
    /// Shared with the connector so replies reuse its publish jitter.
    client: Arc<DiscordClient>,
    /// Re-wrapped copy of the config's bot token. The gateway shard and
    /// the interaction-defer HTTP client both take an owned `String`,
    /// so the envelope is only opened inside [`ChatSource::run`].
    bot_token: SecretBox<String>,
    application_id: u64,
    guild_id: Option<u64>,
    enable_message_content: bool,
    enable_direct_messages: bool,
    enable_reactions: bool,
    commands: Vec<SlashCommandConfig>,
}

impl DiscordChatSource {
    /// Build the source from the connector's config.
    pub fn new(config: &DiscordConfig, client: Arc<DiscordClient>) -> Self {
        // SECURITY: expose needed to re-wrap the token into this source's
        // own `SecretBox`; the plaintext never leaves this expression.
        let token = secrecy::ExposeSecret::expose_secret(&config.bot_token).clone();
        Self {
            client,
            bot_token: SecretBox::new(Box::new(token)),
            application_id: config.application_id,
            guild_id: config.guild_id,
            enable_message_content: config.enable_message_content,
            enable_direct_messages: config.enable_direct_messages,
            enable_reactions: config.enable_reactions,
            commands: config.commands.clone(),
        }
    }

    /// Gateway intents — minimum required, with opt-in privacy flags.
    fn intents(&self) -> Intents {
        let mut intents = Intents::GUILDS;

        if self.enable_message_content {
            // WARNING: This lets the bot read ALL messages in ALL channels.
            tracing::warn!(
                "MESSAGE_CONTENT privileged intent enabled — bot can read ALL channel messages"
            );
            intents |= Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;
        }
        if self.enable_direct_messages {
            intents |= Intents::DIRECT_MESSAGES;
        }
        if self.enable_reactions {
            intents |= Intents::GUILD_MESSAGE_REACTIONS;
        }

        intents
    }

    /// Register the configured slash commands before the shard connects.
    async fn register_commands(&self) -> Result<(), ConnectorError> {
        if self.commands.is_empty() {
            return Ok(());
        }

        self.client
            .register_commands(self.application_id, self.guild_id, &self.commands)
            .await
            .map_err(ConnectorError::from)?;

        match self.guild_id {
            Some(guild_id) => tracing::info!(
                guild_id = guild_id,
                count = self.commands.len(),
                "registered guild slash commands"
            ),
            None => tracing::info!(
                count = self.commands.len(),
                "registered global slash commands (may take up to 1 hour to propagate)"
            ),
        }
        Ok(())
    }
}

#[async_trait]
impl ChatSource for DiscordChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        let intents = self.intents();

        self.register_commands().await?;

        // SECURITY: expose needed for twilight gateway shard and HTTP
        // client initialization — both take an owned `String`, not a
        // `Secret<T>`.
        let token = secrecy::ExposeSecret::expose_secret(&self.bot_token).clone();

        // Interaction defers are answered over their own HTTP client so
        // the connector's publish jitter never delays the 3s deadline.
        let http_client = Arc::new(twilight_http::Client::new(token.clone()));

        let shard = twilight_gateway::Shard::new(twilight_gateway::ShardId::ONE, token, intents);
        tracing::info!("Discord gateway shard created");

        // Dispatcher: routed gateway payloads → ChatMessage. The
        // rule-engine event the gateway classified travels on the message
        // itself (`with_classified_event` reads the payload's "trigger"
        // field) rather than through a separate emit.
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
                        .get("content")
                        .and_then(|c| c.as_str())
                        .or_else(|| payload.get("command_name").and_then(|c| c.as_str()))
                        .unwrap_or("")
                        .to_owned();

                    let message =
                        ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, payload)
                            .with_classified_event();
                    if let Err(e) = tx.send(message).await {
                        tracing::error!(error = %e, "failed to forward Discord message");
                    }
                });
            });

        tracing::info!("Discord gateway starting");
        crate::gateway::gateway_loop(
            shard,
            http_client,
            self.application_id,
            dispatcher,
            shutdown,
        )
        .await;
        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        // Same path as the `send_message` action, including its
        // WorkspaceKey-URI parsing of the channel id.
        let input = serde_json::json!({ "chat_id": channel_id, "text": text });
        actions::send_message::execute(self.client.as_ref(), &input)
            .await
            .map_err(ConnectorError::from)?;
        Ok(())
    }
}
