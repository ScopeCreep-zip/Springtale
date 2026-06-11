use std::sync::Arc;

use anyhow::Context;
use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;

/// Start the Discord gateway.
///
/// The connector is already registered in the registry by the factory system.
/// This function builds gateway intents, registers slash commands, creates a
/// gateway shard, and starts the event loop that bridges Discord events to
/// the bot runtime.
pub async fn wire_discord(
    config: &connector_discord::DiscordConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    trigger_tx: mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
) -> anyhow::Result<tokio::sync::watch::Sender<bool>> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-discord").is_none() {
            anyhow::bail!("connector-discord not found in registry — check config");
        }
    }

    // Build intents — minimum required, with opt-in privacy flags
    let mut intents = twilight_model::gateway::Intents::GUILDS;

    if config.enable_message_content {
        // WARNING: This lets the bot read ALL messages in ALL channels.
        tracing::warn!(
            "MESSAGE_CONTENT privileged intent enabled — bot can read ALL channel messages"
        );
        intents |= twilight_model::gateway::Intents::GUILD_MESSAGES
            | twilight_model::gateway::Intents::MESSAGE_CONTENT;
    }
    if config.enable_direct_messages {
        intents |= twilight_model::gateway::Intents::DIRECT_MESSAGES;
    }
    if config.enable_reactions {
        intents |= twilight_model::gateway::Intents::GUILD_MESSAGE_REACTIONS;
    }

    // 3. Create HTTP client for command registration and interaction responses
    // SECURITY: expose needed for twilight HTTP client initialization
    let token = secrecy::ExposeSecret::expose_secret(&config.bot_token).clone();
    let http_client = Arc::new(twilight_http::Client::new(token.clone()));

    // 4. Register slash commands (if configured)
    if !config.commands.is_empty() {
        let app_id = twilight_model::id::Id::new(config.application_id);

        let tw_commands: Vec<twilight_model::application::command::Command> = config
            .commands
            .iter()
            .map(|cmd| twilight_model::application::command::Command {
                application_id: Some(app_id),
                contexts: None,
                default_member_permissions: None,
                #[allow(deprecated)]
                dm_permission: None,
                description: cmd.description.clone(),
                description_localizations: None,
                guild_id: None,
                id: None,
                integration_types: None,
                kind: twilight_model::application::command::CommandType::ChatInput,
                name: cmd.name.clone(),
                name_localizations: None,
                nsfw: None,
                options: Vec::new(),
                version: twilight_model::id::Id::new(1),
            })
            .collect();

        if let Some(guild_id) = config.guild_id {
            let guild = twilight_model::id::Id::new(guild_id);
            let _: twilight_http::Response<
                twilight_http::response::marker::ListBody<
                    twilight_model::application::command::Command,
                >,
            > = http_client
                .interaction(app_id)
                .set_guild_commands(guild, &tw_commands)
                .await
                .context("failed to register guild slash commands")?;
            tracing::info!(
                guild_id = guild_id,
                count = config.commands.len(),
                "registered guild slash commands"
            );
        } else {
            let _: twilight_http::Response<
                twilight_http::response::marker::ListBody<
                    twilight_model::application::command::Command,
                >,
            > = http_client
                .interaction(app_id)
                .set_global_commands(&tw_commands)
                .await
                .context("failed to register global slash commands")?;
            tracing::info!(
                count = config.commands.len(),
                "registered global slash commands (may take up to 1 hour to propagate)"
            );
        }
    }

    // 5. Create gateway shard
    // SECURITY: expose needed for twilight gateway shard initialization (token already exposed above)
    let shard = twilight_gateway::Shard::new(twilight_gateway::ShardId::ONE, token, intents);
    tracing::info!("Discord gateway shard created");

    // 6. Dispatcher: Discord events → IncomingMessage
    let evt_tx = trigger_tx.clone();
    let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |payload: serde_json::Value| {
            // Rule path: emit the gateway-classified ConnectorEvent so Discord
            // event recipes fire on gateway events, not just the bot chat path.
            super::events::emit_classified(&evt_tx, "connector-discord", &payload);
            let tx = bot_msg_tx.clone();
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
                let text = payload
                    .get("content")
                    .and_then(|c| c.as_str())
                    .or_else(|| payload.get("command_name").and_then(|c| c.as_str()))
                    .unwrap_or("")
                    .to_owned();

                let incoming = springtale_bot::IncomingMessage {
                    user_id,
                    channel_id,
                    text,
                    source_connector: "connector-discord".to_owned(),
                    raw,
                };
                if let Err(e) = tx.send(incoming).await {
                    tracing::error!(error = %e, "failed to send Discord message to bot");
                }
            });
        });

    // 7. Start gateway loop with shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let application_id = config.application_id;

    tokio::spawn(async move {
        connector_discord::gateway::gateway_loop(
            shard,
            http_client,
            application_id,
            dispatcher,
            shutdown_rx,
        )
        .await;
    });

    tracing::info!("Discord gateway started");
    Ok(shutdown_tx)
}
