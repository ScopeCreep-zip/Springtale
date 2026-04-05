use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;

/// Start the Signal SSE gateway.
///
/// The connector is already registered in the registry by the factory system.
/// This function starts an SSE listener against the signal-cli daemon HTTP API
/// and bridges incoming messages to the bot runtime.
///
/// The signal-cli daemon must be started separately by the user:
/// `signal-cli -a +NUMBER daemon --http localhost:PORT`
pub async fn wire_signal(
    config: &connector_signal::SignalConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
) -> anyhow::Result<tokio::sync::watch::Sender<bool>> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-signal").is_none() {
            anyhow::bail!("connector-signal not found in registry — check config");
        }
    }

    let daemon_url = config.daemon_url.clone();

    // 2. Dispatcher: Signal events → IncomingMessage
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
                let text = payload
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_owned();

                let incoming = springtale_bot::IncomingMessage {
                    user_id,
                    channel_id,
                    text,
                    source_connector: "connector-signal".to_owned(),
                    raw,
                };
                if let Err(e) = tx.send(incoming).await {
                    tracing::error!(error = %e, "failed to send Signal message to bot");
                }
            });
        });

    // 3. Start SSE gateway with shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        connector_signal::gateway::gateway_loop(daemon_url, dispatcher, shutdown_rx).await;
    });

    tracing::info!("Signal gateway started (SSE listener)");
    Ok(shutdown_tx)
}
