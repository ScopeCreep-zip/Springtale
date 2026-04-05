use std::sync::Arc;

use anyhow::Context;
use secrecy::ExposeSecret;
use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;

/// Start the Telegram polling gateway.
///
/// The connector is already registered in the registry by the factory system.
/// This function only starts the long-polling loop that bridges incoming
/// Telegram messages to the bot runtime via bot_msg_tx.
pub async fn wire_telegram(
    config: &connector_telegram::TelegramConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
) -> anyhow::Result<()> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-telegram").is_none() {
            anyhow::bail!("connector-telegram not found in registry — check config");
        }
    }

    // Create a separate polling client (the connector instance is owned by the registry)
    // SECURITY: expose needed to create polling client with same token
    let poll_token = secrecy::SecretBox::new(Box::new(config.bot_token.expose_secret().clone()));
    let poll_client = connector_telegram::TelegramClient::new(&config.api_base, poll_token)
        .context("failed to create Telegram polling client")?;

    let poll_client = Arc::new(poll_client);
    let poll_timeout = config.poll_timeout;

    // Polling dispatcher: extracts message fields from Telegram updates
    // and sends IncomingMessage to the bot via bot_msg_tx.
    let poll_dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |update: serde_json::Value| {
            if let Some(message) = update.get("message") {
                let tx = bot_msg_tx.clone();
                let msg = message.clone();
                let raw = update.clone();
                tokio::spawn(async move {
                    let user_id = msg
                        .get("from")
                        .and_then(|f| f.get("id"))
                        .and_then(|i| i.as_i64())
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    let channel_id = msg
                        .get("chat")
                        .and_then(|c| c.get("id"))
                        .and_then(|i| i.as_i64())
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    let text = msg
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_owned();

                    let incoming = springtale_bot::IncomingMessage {
                        user_id,
                        channel_id,
                        text,
                        source_connector: "connector-telegram".to_owned(),
                        raw,
                    };
                    if let Err(e) = tx.send(incoming).await {
                        tracing::error!(error = %e, "failed to send Telegram message to bot");
                    }
                });
            }
        });

    let (_poll_shutdown_tx, poll_shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        connector_telegram::polling::polling_loop(
            poll_client,
            poll_timeout,
            vec![],
            poll_dispatcher,
            poll_shutdown_rx,
        )
        .await;
    });

    tracing::info!("Telegram polling started");
    Ok(())
}
