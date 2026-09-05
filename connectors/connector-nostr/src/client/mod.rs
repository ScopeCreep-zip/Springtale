use std::time::Duration;

use async_trait::async_trait;
use nostr_sdk::prelude::*;

use crate::error::NostrError;

/// A Nostr pubkey discovered via active enumeration.
///
/// Pubkey is hex-encoded — matches the URI scheme
/// `nostr://pubkey/{hex}`.
#[derive(Debug, Clone)]
pub struct DiscoveredNostrPubkey {
    pub pubkey_hex: String,
    pub alias: Option<String>,
}

/// Trait defining the Nostr API surface used by actions.
/// Actions depend on this trait, not the concrete client — enables mock testing.
#[async_trait]
pub trait NostrApi: Send + Sync {
    /// Publish a text note (kind 1).
    async fn publish_note(&self, content: &str) -> Result<String, NostrError>;

    /// Send an encrypted DM via NIP-17 (NIP-44 gift wrap).
    async fn send_dm(&self, recipient_pubkey: &str, content: &str) -> Result<String, NostrError>;

    /// React to an event (kind 7).
    async fn react(&self, event_id: &str, reaction: &str) -> Result<String, NostrError>;

    /// Reply to a note (kind 1 with e/p tags).
    async fn reply(&self, event_id: &str, content: &str) -> Result<String, NostrError>;

    /// Fetch the bot's NIP-02 Kind 3 contact list from configured relays
    /// and enumerate its `p` tags.
    async fn list_destinations(&self) -> Result<Vec<DiscoveredNostrPubkey>, NostrError>;
}

/// Concrete Nostr client backed by nostr-sdk.
///
/// Applies publish-side jitter (Fix 3) to obscure activity timing
/// from relay observers (ARCHITECTURE.md §2.9 social graph protection).
pub struct NostrClient {
    inner: Client,
    /// Signing keys. nostr-sdk 0.45 decouples the relay client from the
    /// signer, so every event is finalized (signed / encrypted) locally
    /// before it is handed to the relay pool.
    keys: Keys,
    /// Jitter in seconds applied BEFORE publishing to relays.
    /// This hides the exact time the bot decided to act from relay observers.
    jitter_secs: u64,
}

impl NostrClient {
    /// Create a new NostrClient from parsed keys and relay URLs.
    pub async fn new(
        keys: Keys,
        relay_urls: &[String],
        jitter_secs: u64,
    ) -> Result<Self, NostrError> {
        // NIP-42 relay AUTH challenges are answered with the bot's own keys.
        let client = Client::builder()
            .authenticator(SignerAuthenticator::new(keys.clone()))
            .build();

        for url in relay_urls {
            client
                .add_relay(url.as_str())
                .await
                .map_err(|e| NostrError::RelayError(format!("failed to add relay {url}: {e}")))?;
        }

        client.connect().await;
        tracing::info!(relays = relay_urls.len(), "connected to Nostr relays");

        Ok(Self {
            inner: client,
            keys,
            jitter_secs,
        })
    }

    /// Get a reference to the inner nostr-sdk Client (for gateway subscription).
    pub fn inner(&self) -> &Client {
        &self.inner
    }

    /// The bot's signing keys (the gateway needs them to unwrap NIP-59 gift wraps).
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Apply publish-side jitter before sending events to relays.
    /// Delays by a random 0..jitter_secs to prevent timing correlation.
    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(Duration::from_secs(jitter)).await;
        }
    }

    /// Fetch a single event by ID from the connected relays.
    async fn fetch_event(&self, event_id: &str) -> Result<Event, NostrError> {
        let eid = EventId::parse(event_id)
            .map_err(|e| NostrError::InvalidInput(format!("invalid event ID: {e}")))?;
        let filter = Filter::new().id(eid).limit(1);
        let events = self
            .inner
            .fetch_events(filter)
            .timeout(Duration::from_secs(5))
            .await
            .map_err(|e| NostrError::RelayError(format!("failed to fetch event: {e}")))?;
        events
            .into_iter()
            .next()
            .ok_or_else(|| NostrError::InvalidInput(format!("event not found: {event_id}")))
    }

    /// Broadcast an already-signed event and return its hex ID.
    async fn broadcast(&self, event: &Event, what: &str) -> Result<String, NostrError> {
        let output = self
            .inner
            .send_event(event)
            .await
            .map_err(|e| NostrError::PublishFailed(format!("failed to {what}: {e}")))?;
        Ok(output.value.to_hex())
    }
}

#[async_trait]
impl NostrApi for NostrClient {
    async fn publish_note(&self, content: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        let event = EventBuilder::new(Kind::TextNote, content)
            .finalize(&self.keys)
            .map_err(|e| NostrError::PublishFailed(format!("failed to sign note: {e}")))?;
        self.broadcast(&event, "publish note").await
    }

    async fn send_dm(&self, recipient_pubkey: &str, content: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        let pubkey = PublicKey::parse(recipient_pubkey)
            .map_err(|e| NostrError::InvalidInput(format!("invalid pubkey: {e}")))?;

        // NIP-17 private DM: the rumor is sealed with NIP-44 and gift-wrapped
        // (NIP-59) for the recipient. NIP-04 is deprecated and never used.
        // The config.dm_encryption field documents this choice but doesn't
        // change behavior: NIP-44 is always used per spec requirement.
        let event = PrivateDirectMessageBuilder::new(pubkey, content)
            .finalize(&self.keys)
            .map_err(|e| NostrError::EncryptionError(format!("failed to seal DM: {e}")))?;
        self.broadcast(&event, "send DM").await
    }

    async fn react(&self, event_id: &str, reaction: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        // Fetch the target event to build proper reaction tags
        let target = self.fetch_event(event_id).await?;
        let event = ReactionBuilder::new(ReactionTarget::new(&target, None), reaction)
            .finalize(&self.keys)
            .map_err(|e| NostrError::PublishFailed(format!("failed to sign reaction: {e}")))?;
        self.broadcast(&event, "react").await
    }

    async fn reply(&self, event_id: &str, content: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        // Fetch the target event for proper reply tags
        let target = self.fetch_event(event_id).await?;
        let event = TextNoteReplyBuilder::new(content, &target)
            .finalize(&self.keys)
            .map_err(|e| NostrError::PublishFailed(format!("failed to sign reply: {e}")))?;
        self.broadcast(&event, "reply").await
    }

    async fn list_destinations(&self) -> Result<Vec<DiscoveredNostrPubkey>, NostrError> {
        // NIP-02: the bot's latest kind-3 contact list, one `p` tag per contact
        // (["p", <pubkey>, <relay?>, <petname?>]).
        let filter = Filter::new()
            .author(self.keys.public_key())
            .kind(Kind::ContactList)
            .limit(1);
        let events = self
            .inner
            .fetch_events(filter)
            .timeout(Duration::from_secs(5))
            .await
            .map_err(|e| NostrError::RelayError(format!("failed to fetch contact list: {e}")))?;
        let out = events
            .iter()
            .flat_map(|e| e.tags.iter())
            .filter_map(|tag| {
                let parts = tag.as_slice();
                if parts.first().map(String::as_str) != Some("p") {
                    return None;
                }
                let pubkey_hex = PublicKey::parse(parts.get(1)?).ok()?.to_hex();
                let alias = parts.get(3).filter(|s| !s.is_empty()).cloned();
                Some(DiscoveredNostrPubkey { pubkey_hex, alias })
            })
            .collect();
        Ok(out)
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockNostrApi {
        pub response_id: String,
    }

    #[async_trait]
    impl NostrApi for MockNostrApi {
        async fn publish_note(&self, _: &str) -> Result<String, NostrError> {
            Ok(self.response_id.clone())
        }
        async fn send_dm(&self, _: &str, _: &str) -> Result<String, NostrError> {
            Ok(self.response_id.clone())
        }
        async fn react(&self, _: &str, _: &str) -> Result<String, NostrError> {
            Ok(self.response_id.clone())
        }
        async fn reply(&self, _: &str, _: &str) -> Result<String, NostrError> {
            Ok(self.response_id.clone())
        }
        async fn list_destinations(&self) -> Result<Vec<DiscoveredNostrPubkey>, NostrError> {
            Ok(vec![
                DiscoveredNostrPubkey {
                    pubkey_hex: "0".repeat(64),
                    alias: Some("alice".to_owned()),
                },
                DiscoveredNostrPubkey {
                    pubkey_hex: "1".repeat(64),
                    alias: None,
                },
                DiscoveredNostrPubkey {
                    pubkey_hex: "2".repeat(64),
                    alias: Some("bob".to_owned()),
                },
            ])
        }
    }
}
