//! Chat ingestion for Nostr.
//!
//! This is the relay subscription loop the daemon used to own
//! (`wire_nostr`), moved into the crate that owns the protocol. The
//! runtime now only starts and stops it, keyed off the registry, so a
//! Nostr connector installed at runtime receives DMs immediately.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use springtale_connector::chat::{ChatMessage, ChatSource};
use springtale_connector::error::ConnectorError;

use crate::client::{NostrApi, NostrClient};
use crate::config::NostrConfig;

/// Connector name stamped on every emitted [`ChatMessage`].
const CONNECTOR_NAME: &str = "connector-nostr";

/// Length of a hex-encoded secp256k1 x-only public key.
const PUBKEY_HEX_LEN: usize = 64;

/// The Nostr connector's inbound/outbound chat half.
///
/// Holds the connector's own authenticated client for the outbound
/// side, and the relay list needed to open the *gateway* client that
/// [`ChatSource::run`] subscribes with. The two are deliberately
/// separate: [`crate::gateway::gateway_loop`] unsubscribes and
/// disconnects its client when it stops, which must not take the
/// connector's action client down with it.
pub struct NostrChatSource {
    /// Outbound half — the connector's authenticated client, shared so
    /// a reply neither re-parses the private key nor opens a third
    /// relay connection.
    client: Arc<NostrClient>,
    /// Relay URLs the gateway client connects to.
    relays: Vec<String>,
    /// Publish-side jitter, carried through to the gateway client.
    jitter_secs: u64,
}

impl NostrChatSource {
    /// Build the chat half from the connector's client and config.
    ///
    /// The private key is never re-read here: the keys already live in
    /// `client` (parsed once, through `springtale_crypto::secret_use`,
    /// in [`crate::auth::parse_keys`]).
    pub fn new(client: Arc<NostrClient>, config: &NostrConfig) -> Self {
        Self {
            client,
            relays: config.relays.clone(),
            jitter_secs: config.message_jitter_secs,
        }
    }
}

#[async_trait]
impl ChatSource for NostrChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        // A separate client for the subscription loop: gateway_loop owns
        // its client's lifetime (unsubscribe_all + disconnect on stop).
        let gateway =
            NostrClient::new(self.client.keys().clone(), &self.relays, self.jitter_secs).await?;

        let gateway_client = Arc::new(gateway.inner().clone());
        // nostr-sdk 0.45 decouples the relay client from the signer: the
        // gateway needs the bot's keys itself to unwrap NIP-59
        // gift-wrapped DMs.
        let gateway_keys = gateway.keys().clone();
        let bot_pubkey = gateway_keys.public_key();

        tracing::info!(
            pubkey = %bot_pubkey.to_hex(),
            "Nostr gateway client ready"
        );

        // Dispatcher: Nostr relay events → ChatMessage. The rule-engine
        // fan-out the daemon did inline (`emit_classified`) is now
        // `with_classified_event()`, read from the payload's own
        // `"trigger"` field by the runtime's chat wiring.
        let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
            Arc::new(move |payload: serde_json::Value| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let user_id = payload
                        .get("pubkey")
                        .or_else(|| payload.get("sender_pubkey"))
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_owned();
                    let text = payload
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_owned();
                    // The channel is the correspondent's pubkey, not the
                    // relay URL the daemon used: a Nostr reply is a
                    // NIP-17 DM addressed to a key. A relay URL is not
                    // addressable, and answering a private DM with a
                    // public note is exactly the failure this project
                    // exists to prevent. The relay URL stays in `raw`.
                    let channel_id = user_id.clone();

                    let msg = ChatMessage::chat(CONNECTOR_NAME, channel_id, user_id, text, payload)
                        .with_classified_event();
                    if let Err(e) = tx.send(msg).await {
                        tracing::error!(error = %e, "failed to forward Nostr message");
                    }
                });
            });

        crate::gateway::gateway_loop(
            gateway_client,
            gateway_keys,
            bot_pubkey,
            dispatcher,
            shutdown,
        )
        .await;

        Ok(())
    }

    async fn send(&self, channel_id: &str, text: &str) -> Result<(), ConnectorError> {
        if channel_id.is_empty() {
            return Err(ConnectorError::ExecutionFailed(
                "nostr reply needs a recipient pubkey".to_owned(),
            ));
        }
        // Accepts a raw pubkey/npub or a `nostr://pubkey/{hex}` URI.
        let recipient =
            springtale_connector::workspace_key::extract_id_for_scheme(channel_id, CONNECTOR_NAME)
                .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;

        let is_pubkey = recipient.starts_with("npub1")
            || (recipient.len() == PUBKEY_HEX_LEN
                && recipient.chars().all(|c| c.is_ascii_hexdigit()));
        if !is_pubkey {
            // Never fall back to a public note: a chat reply is private.
            return Err(ConnectorError::ExecutionFailed(format!(
                "not a nostr pubkey, refusing to publish a chat reply publicly: {channel_id}"
            )));
        }

        self.client.send_dm(recipient, text).await?;
        Ok(())
    }
}
