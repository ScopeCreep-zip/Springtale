use std::sync::Arc;

use anyhow::Context;
use tokio::sync::{RwLock, mpsc, watch};

use connector_bluesky::client::BlueskyApi;
use springtale_connector::registry::store::ConnectorRegistry;

/// Start the Bluesky Jetstream firehose gateway.
///
/// The connector itself (outbound actions: create_post/reply/…) is already
/// registered in the registry by the factory system. This wires the
/// INBOUND side: a Jetstream subscription that fires ConnectorEvent
/// recipes on the account's own posts (`own_post`) and on posts that
/// @mention the account (`mention`). Bluesky firehose events are
/// automation triggers, not interactive chat, so they route to the rule
/// engine only (not `bot_msg_tx`).
///
/// Returns a shutdown sender for graceful stop.
pub async fn wire_bluesky(
    config: &connector_bluesky::BlueskyConfig,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    trigger_tx: mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
) -> anyhow::Result<watch::Sender<bool>> {
    {
        let reg = registry.read().await;
        if reg.get("connector-bluesky").is_none() {
            anyhow::bail!("connector-bluesky not found in registry — check config");
        }
    }

    // Authenticate a gateway client to learn our own DID — needed to
    // classify a firehose post as own_post (author == us) vs mention
    // (a facet#mention referencing us).
    let client = connector_bluesky::client::AtProtoClient::new(config)
        .await
        .context("failed to authenticate Bluesky gateway client")?;
    let (own_did, handle) = client
        .current_account()
        .await
        .context("failed to resolve Bluesky account DID")?;
    tracing::info!(handle = %handle, did = %own_did, "Bluesky Jetstream gateway client ready");

    let evt_tx = trigger_tx.clone();
    let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |payload: serde_json::Value| {
            super::events::emit_classified(&evt_tx, "connector-bluesky", &payload);
        });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let jetstream_url = config.jetstream_url.clone();
    tokio::spawn(async move {
        connector_bluesky::gateway::gateway_loop(jetstream_url, own_did, dispatcher, shutdown_rx)
            .await;
    });

    Ok(shutdown_tx)
}
