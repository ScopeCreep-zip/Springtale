use std::sync::Arc;

use nostr_sdk::prelude::*;

/// Run the Nostr relay subscription loop.
///
/// Subscribes to text notes, gift-wrapped DMs, mentions, and reactions
/// for the bot's public key. Dispatches events with trigger routing
/// and DM decryption (Fix 1+2).
///
/// Runs until shutdown signal received.
pub async fn gateway_loop(
    client: Arc<nostr_sdk::Client>,
    bot_pubkey: PublicKey,
    dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    // Subscribe to relevant events for this bot's pubkey
    let filter = Filter::new()
        .kinds([Kind::TextNote, Kind::GiftWrap, Kind::Reaction])
        .pubkey(bot_pubkey)
        .limit(0); // 0 = only new events going forward

    if let Err(e) = client.subscribe(filter, None).await {
        tracing::error!(error = %e, "failed to subscribe to Nostr events");
        return;
    }

    tracing::info!("Nostr gateway subscribed to events");

    let mut rx = client.notifications();

    loop {
        tokio::select! {
            Ok(notification) = rx.recv() => {
                match notification {
                    RelayPoolNotification::Event { event, relay_url, .. } => {
                        // Fix 2: Route by event kind to specific triggers
                        let payload = route_event(
                            &client,
                            &event,
                            &relay_url,
                            &bot_pubkey,
                        ).await;

                        if let Some(p) = payload {
                            dispatcher(p);
                        }
                    }
                    RelayPoolNotification::Shutdown => {
                        tracing::info!("Nostr relay pool shutting down");
                        break;
                    }
                    _ => {}
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Nostr gateway received shutdown signal");
                    break;
                }
            }
        }
    }

    client.unsubscribe_all().await;
    client.disconnect().await;
    tracing::info!("Nostr gateway stopped");
}

/// Route an event to the correct trigger based on kind.
/// Returns None if the event should be ignored.
async fn route_event(
    client: &nostr_sdk::Client,
    event: &Event,
    relay_url: &RelayUrl,
    bot_pubkey: &PublicKey,
) -> Option<serde_json::Value> {
    match event.kind {
        // Fix 1: Decrypt gift-wrapped DMs (NIP-17 via NIP-44)
        Kind::GiftWrap => {
            match client.unwrap_gift_wrap(event).await {
                Ok(gift) => {
                    Some(serde_json::json!({
                        "trigger": "dm_received",
                        "sender_pubkey": gift.sender.to_hex(),
                        "content": gift.rumor.content,
                        "created_at": event.created_at.as_secs(),
                        "relay_url": relay_url.to_string(),
                        // pubkey field used by dispatcher for user_id
                        "pubkey": gift.sender.to_hex(),
                    }))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to unwrap gift-wrapped DM");
                    None
                }
            }
        }

        // Text notes — check if it mentions the bot (p-tag)
        Kind::TextNote => {
            let is_mention = event.tags.iter().any(|tag| {
                let parts = tag.as_slice();
                parts.first().map(|s| s.as_str()) == Some("p")
                    && parts.get(1).map(|s| s.as_str()) == Some(&bot_pubkey.to_hex())
            });

            let trigger = if is_mention {
                "mention_received"
            } else {
                "note_received"
            };

            Some(build_event_payload(trigger, event, relay_url))
        }

        // Reactions (kind 7)
        Kind::Reaction => {
            let mut payload = build_event_payload("reaction_received", event, relay_url);
            // Extract target event ID from e-tag
            if let Some(target) = event.tags.iter().find_map(|tag| {
                let parts = tag.as_slice();
                if parts.first().map(|s| s.as_str()) == Some("e") {
                    parts.get(1).map(|s| s.to_owned())
                } else {
                    None
                }
            }) && let Some(obj) = payload.as_object_mut()
            {
                obj.insert(
                    "target_event_id".to_owned(),
                    serde_json::Value::String(target),
                );
            }
            Some(payload)
        }

        // Unknown kinds — ignore
        _ => None,
    }
}

/// Build a standard event payload with common fields.
fn build_event_payload(trigger: &str, event: &Event, relay_url: &RelayUrl) -> serde_json::Value {
    serde_json::json!({
        "trigger": trigger,
        "event_id": event.id.to_hex(),
        "pubkey": event.pubkey.to_hex(),
        "kind": event.kind.as_u16(),
        "content": event.content,
        "created_at": event.created_at.as_secs(),
        "relay_url": relay_url.to_string(),
        "tags": event.tags.iter().map(|t| {
            serde_json::Value::Array(
                t.as_slice().iter().map(|s| serde_json::Value::String(s.to_owned())).collect()
            )
        }).collect::<Vec<_>>(),
    })
}
