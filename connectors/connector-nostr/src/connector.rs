use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};
use springtale_connector::{Subscription, SubscriptionCounter, SubscriptionId};

use crate::actions;
use crate::client::NostrClient;
use crate::config::NostrConfig;
use crate::triggers;
use springtale_connector::manifest::SignatureAlgorithm;

/// Nostr connector.
///
/// Pseudonymous by design — no phone, no email, no KYC required.
/// E2E encrypted DMs via NIP-44/NIP-17. Best fit for Springtale's
/// vulnerable user mission.
///
/// CRITICAL: Uses secp256k1 Schnorr signatures (BIP-340), NOT Ed25519.
pub struct NostrConnector {
    client: NostrClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl NostrConnector {
    pub async fn new(config: &NostrConfig) -> Result<Self, crate::error::NostrError> {
        let keys = crate::auth::parse_keys(&config.private_key)?;
        let client = NostrClient::new(keys, &config.relays, config.message_jitter_secs).await?;

        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        // Build capabilities from configured relay URLs
        let capabilities: Vec<Capability> = config
            .relays
            .iter()
            .filter_map(|url| {
                // Only wss:// accepted — ws:// rejected because unencrypted
                // WebSocket leaks message content to network observers.
                // For IPV survivors, network monitoring is a real threat.
                url.strip_prefix("wss://").map(|host| {
                    let host = host.trim_end_matches('/');
                    Capability::NetworkOutbound {
                        host: host.to_owned(),
                    }
                })
            })
            .collect();

        let manifest = ConnectorManifest {
            name: "connector-nostr".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            author: "Springtale".to_owned(),
            description:
                "Nostr relay connector — pseudonymous notes, NIP-44 encrypted DMs. Best for vulnerable users."
                    .to_owned(),
            capabilities,
            triggers: trigger_decls.clone(),
            actions: action_decls.clone(),
            data_disclosure: vec![
                DataDisclosure {
                    data_type: "public notes (kind 1)".to_owned(),
                    purpose: "posting to Nostr network".to_owned(),
                    destination: "Nostr relays (replicated across network)".to_owned(),
                },
                DataDisclosure {
                    data_type: "encrypted DMs (NIP-44 content)".to_owned(),
                    purpose: "E2E encrypted messaging".to_owned(),
                    destination: "relays see ciphertext; content readable only by recipient"
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "event metadata (pubkeys, timestamps, tags)".to_owned(),
                    purpose: "Nostr protocol requirement — cannot be hidden".to_owned(),
                    destination:
                        "visible to ALL relay operators and network observers".to_owned(),
                },
                DataDisclosure {
                    data_type: "relationship graph (p-tags, e-tags)".to_owned(),
                    purpose: "reply threading and mentions".to_owned(),
                    destination:
                        "visible to relay operators — reveals who communicates with whom"
                            .to_owned(),
                },
            ],
            roles: vec![],
            wasm_hash: None,
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        };

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            sub_counter: SubscriptionCounter::new(),
        })
    }

    /// Get the inner nostr-sdk Client for gateway subscription.
    pub fn nostr_client(&self) -> &NostrClient {
        &self.client
    }
}

#[async_trait]
impl Connector for NostrConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        &self.triggers
    }

    fn actions(&self) -> &[ActionDecl] {
        &self.actions
    }

    async fn execute(
        &self,
        action: &str,
        input: serde_json::Value,
    ) -> Result<ActionResult, ConnectorError> {
        match action {
            "publish_note" => actions::publish_note::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "send_dm" => actions::send_dm::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "react" => actions::react::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "reply" => actions::reply::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "send_message" => actions::send_message::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "discover_destinations" => {
                actions::discover_destinations::execute(&self.client, &input)
                    .await
                    .map_err(ConnectorError::from)
            }
            unknown => Err(ConnectorError::ExecutionFailed(format!(
                "unknown action: {unknown}"
            ))),
        }
    }

    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        let valid = [
            "note_received",
            "dm_received",
            "mention_received",
            "reaction_received",
        ];
        if !valid.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered Nostr event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed Nostr event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn mention_extractor(&self) -> Option<&dyn springtale_connector::mention::MentionExtractor> {
        Some(&crate::mention::NOSTR_MENTION_EXTRACTOR)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(triggers::trigger_declarations().len(), 4);
    }

    #[test]
    fn test_action_count() {
        // 5 messaging actions + D1's `discover_destinations` enumeration.
        assert_eq!(actions::action_declarations().len(), 6);
    }
}
