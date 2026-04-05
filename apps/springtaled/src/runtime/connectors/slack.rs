use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;

/// Start the Slack Socket Mode gateway.
///
/// The connector is already registered in the registry by the factory system.
/// This function starts the WebSocket-based Socket Mode loop that bridges
/// incoming Slack events to the bot runtime.
pub async fn wire_slack(
    config: &connector_slack::SlackConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
) -> anyhow::Result<tokio::sync::watch::Sender<bool>> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-slack").is_none() {
            anyhow::bail!("connector-slack not found in registry — check config");
        }
    }

    // Get app token for Socket Mode gateway
    // SECURITY: expose needed for Socket Mode WebSocket connection
    let app_token = secrecy::ExposeSecret::expose_secret(&config.app_token).clone();

    // 3. Dispatcher: Slack events → IncomingMessage
    let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |payload: serde_json::Value| {
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

                let incoming = springtale_bot::IncomingMessage {
                    user_id,
                    channel_id,
                    text,
                    source_connector: "connector-slack".to_owned(),
                    raw,
                };
                if let Err(e) = tx.send(incoming).await {
                    tracing::error!(error = %e, "failed to send Slack message to bot");
                }
            });
        });

    // 4. Start Socket Mode gateway with shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        connector_slack::gateway::gateway_loop(app_token, dispatcher, shutdown_rx).await;
    });

    tracing::info!("Slack Socket Mode gateway started");
    Ok(shutdown_tx)
}
