use async_trait::async_trait;

use crate::error::NostrError;

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
}

/// Concrete Nostr client wrapping nostr-sdk.
/// All relay communication goes through this client.
///
/// Applies publish-side jitter (Fix 3) to obscure activity timing
/// from relay observers (ARCHITECTURE.md §2.9 social graph protection).
pub struct NostrClient {
    inner: nostr_sdk::Client,
    /// Jitter in seconds applied BEFORE publishing to relays.
    /// This hides the exact time the bot decided to act from relay observers.
    jitter_secs: u64,
}

impl NostrClient {
    /// Create a new NostrClient from parsed keys and relay URLs.
    pub async fn new(
        keys: nostr_sdk::Keys,
        relay_urls: &[String],
        jitter_secs: u64,
    ) -> Result<Self, NostrError> {
        let client = nostr_sdk::Client::new(keys);

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
            jitter_secs,
        })
    }

    /// Get a reference to the inner nostr-sdk Client (for gateway subscription).
    pub fn inner(&self) -> &nostr_sdk::Client {
        &self.inner
    }

    /// Apply publish-side jitter before sending events to relays.
    /// Delays by a random 0..jitter_secs to prevent timing correlation.
    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;
        }
    }
}

#[async_trait]
impl NostrApi for NostrClient {
    async fn publish_note(&self, content: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        let builder = nostr_sdk::EventBuilder::text_note(content);
        let output = self
            .inner
            .send_event_builder(builder)
            .await
            .map_err(|e| NostrError::PublishFailed(format!("failed to publish note: {e}")))?;
        Ok(output.val.to_hex())
    }

    async fn send_dm(&self, recipient_pubkey: &str, content: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        let pubkey = nostr_sdk::PublicKey::parse(recipient_pubkey)
            .map_err(|e| NostrError::InvalidInput(format!("invalid pubkey: {e}")))?;

        // send_private_msg uses NIP-17 (gift-wrapped NIP-44 encryption) when
        // the `nip59` feature is enabled (which it is in our Cargo.toml).
        // This is the modern, secure DM method — NIP-04 is deprecated.
        // The config.dm_encryption field documents this choice but doesn't
        // change behavior: NIP-44 is always used per spec requirement.
        let output = self
            .inner
            .send_private_msg(pubkey, content, [])
            .await
            .map_err(|e| NostrError::EncryptionError(format!("failed to send DM: {e}")))?;
        Ok(output.val.to_hex())
    }

    async fn react(&self, event_id: &str, reaction: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        let eid = nostr_sdk::EventId::parse(event_id)
            .map_err(|e| NostrError::InvalidInput(format!("invalid event ID: {e}")))?;

        // Fetch the target event to build proper reaction tags
        let filter = nostr_sdk::Filter::new().id(eid).limit(1);
        let events = self
            .inner
            .fetch_events(filter, std::time::Duration::from_secs(5))
            .await
            .map_err(|e| NostrError::RelayError(format!("failed to fetch event: {e}")))?;

        let target = events
            .first()
            .ok_or_else(|| NostrError::InvalidInput(format!("event not found: {event_id}")))?;

        let builder = nostr_sdk::EventBuilder::reaction(target, reaction);
        let output = self
            .inner
            .send_event_builder(builder)
            .await
            .map_err(|e| NostrError::PublishFailed(format!("failed to react: {e}")))?;
        Ok(output.val.to_hex())
    }

    async fn reply(&self, event_id: &str, content: &str) -> Result<String, NostrError> {
        self.apply_jitter().await;
        let eid = nostr_sdk::EventId::parse(event_id)
            .map_err(|e| NostrError::InvalidInput(format!("invalid event ID: {e}")))?;

        // Fetch the target event for proper reply tags
        let filter = nostr_sdk::Filter::new().id(eid).limit(1);
        let events = self
            .inner
            .fetch_events(filter, std::time::Duration::from_secs(5))
            .await
            .map_err(|e| NostrError::RelayError(format!("failed to fetch event: {e}")))?;

        let target = events
            .first()
            .ok_or_else(|| NostrError::InvalidInput(format!("event not found: {event_id}")))?;

        let builder = nostr_sdk::EventBuilder::text_note_reply(content, target, None, None);
        let output = self
            .inner
            .send_event_builder(builder)
            .await
            .map_err(|e| NostrError::PublishFailed(format!("failed to reply: {e}")))?;
        Ok(output.val.to_hex())
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
    }
}
