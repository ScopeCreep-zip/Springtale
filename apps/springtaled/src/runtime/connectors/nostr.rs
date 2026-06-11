use std::sync::Arc;

use anyhow::Context;
use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;

/// Start the Nostr relay subscription gateway.
///
/// The connector is already registered in the registry by the factory system.
/// This function creates a separate Nostr client for the gateway loop,
/// extracts the bot pubkey, and subscribes to relay events.
pub async fn wire_nostr(
    config: &connector_nostr::NostrConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    trigger_tx: mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
) -> anyhow::Result<tokio::sync::watch::Sender<bool>> {
    // Verify connector was registered by factory
    {
        let reg = registry.read().await;
        if reg.get("connector-nostr").is_none() {
            anyhow::bail!("connector-nostr not found in registry — check config");
        }
    }

    // Create a separate Nostr client for the gateway (the connector's client
    // is owned by the registry). This client connects to the same relays.
    let gateway_connector = connector_nostr::NostrConnector::new(config)
        .await
        .context("failed to create Nostr gateway client")?;

    let gateway_client = Arc::new(gateway_connector.nostr_client().inner().clone());
    let bot_pubkey = {
        let signer = gateway_client
            .signer()
            .await
            .map_err(|e| anyhow::anyhow!("Nostr client has no signer: {e}"))?;
        signer.get_public_key().await.map_err(|e| {
            anyhow::anyhow!(
                "failed to extract Nostr public key from signer — \
                 cannot subscribe to events without the correct pubkey: {e}"
            )
        })?
    };

    tracing::info!(
        pubkey = %bot_pubkey.to_hex(),
        "Nostr gateway client ready"
    );

    // Dispatcher: Nostr events → IncomingMessage
    let evt_tx = trigger_tx.clone();
    let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |payload: serde_json::Value| {
            // Rule path: emit the gateway-classified ConnectorEvent so Nostr
            // event recipes fire on relay events, not just the bot chat path.
            super::events::emit_classified(&evt_tx, "connector-nostr", &payload);
            let tx = bot_msg_tx.clone();
            let raw = payload.clone();
            tokio::spawn(async move {
                let user_id = payload
                    .get("pubkey")
                    .or_else(|| payload.get("sender_pubkey"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_owned();
                let channel_id = payload
                    .get("relay_url")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .to_owned();
                let text = payload
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_owned();

                let incoming = springtale_bot::IncomingMessage {
                    user_id,
                    channel_id,
                    text,
                    source_connector: "connector-nostr".to_owned(),
                    raw,
                };
                if let Err(e) = tx.send(incoming).await {
                    tracing::error!(error = %e, "failed to send Nostr message to bot");
                }
            });
        });

    // Start gateway with shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        connector_nostr::gateway::gateway_loop(gateway_client, bot_pubkey, dispatcher, shutdown_rx)
            .await;
    });

    tracing::info!("Nostr gateway started");
    Ok(shutdown_tx)
}
