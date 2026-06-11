use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;

/// Start the IRC gateway loop.
///
/// The connector is already registered in the registry by the factory system.
/// This function builds a separate irc::client::Config for the gateway,
/// starts the reconnection loop, and bridges messages to the bot runtime.
pub async fn wire_irc(
    config: &connector_irc::IrcConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    trigger_tx: mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
) -> anyhow::Result<tokio::sync::watch::Sender<bool>> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-irc").is_none() {
            anyhow::bail!("connector-irc not found in registry — check config");
        }
    }

    // Build irc crate config for the gateway
    let nick_password = config
        .nickserv_password
        .as_ref()
        .map(springtale_crypto::secret_use::header_value);

    let gateway_config = irc::client::data::Config {
        nickname: Some(config.nick.clone()),
        server: Some(config.server.clone()),
        port: Some(config.port),
        use_tls: Some(config.use_tls),
        nick_password,
        channels: config.channels.clone(),
        version: Some(String::new()), // Privacy: disable CTCP VERSION
        burst_window_length: Some(8),
        max_messages_in_burst: Some(15),
        ..irc::client::data::Config::default()
    };

    // 3. Dispatcher: extract IRC fields → IncomingMessage
    let command_prefix = config.command_prefix.clone();
    let evt_tx = trigger_tx.clone();
    let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |payload: serde_json::Value| {
            // Rule path: emit the gateway-classified ConnectorEvent to the
            // engine so IRC event recipes fire on polling, not just the bot.
            super::events::emit_classified(&evt_tx, "connector-irc", &payload);
            let tx = bot_msg_tx.clone();
            let raw = payload.clone();
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

                let incoming = springtale_bot::IncomingMessage {
                    user_id,
                    channel_id,
                    text,
                    source_connector: "connector-irc".to_owned(),
                    raw,
                };
                if let Err(e) = tx.send(incoming).await {
                    tracing::error!(error = %e, "failed to send IRC message to bot");
                }
            });
        });

    // 4. Start gateway loop with shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let sasl_enabled = config.sasl_enabled;
    tokio::spawn(async move {
        connector_irc::gateway::gateway_loop(
            gateway_config,
            command_prefix,
            sasl_enabled,
            dispatcher,
            shutdown_rx,
        )
        .await;
    });

    tracing::info!("IRC gateway started");
    Ok(shutdown_tx)
}
