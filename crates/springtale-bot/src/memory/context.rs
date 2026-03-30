use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::BotError;
use crate::state::session::SessionKey;

/// Sliding window of recent conversation per (user_id, channel_id).
pub struct ConversationContext {
    max_messages: usize,
    store: Arc<dyn StorageBackend>,
}

impl ConversationContext {
    pub fn new(store: Arc<dyn StorageBackend>, max_messages: usize) -> Self {
        Self {
            max_messages,
            store,
        }
    }

    /// Push a new message into the conversation context.
    ///
    /// Phase 1b: content stored as plaintext bytes in `content_encrypted`.
    /// Phase 2 will add vault-based XChaCha20-Poly1305 encryption with the
    /// random nonce already generated here. The nonce is random now so the
    /// schema is migration-ready when encryption is wired in.
    pub async fn push(
        &self,
        key: &SessionKey,
        author: &str,
        content: &str,
    ) -> Result<(), BotError> {
        // Derive source from author role
        let source = match author {
            "user" => "user_input",
            "assistant" => "connector_output",
            _ => "connector_output",
        };

        // Generate random nonce for future encryption readiness
        let nonce = {
            use rand::RngCore;
            let mut n = vec![0u8; 24];
            rand::thread_rng().fill_bytes(&mut n);
            n
        };

        let entry = springtale_store::MemoryRow {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: key.user_id.clone(),
            channel_id: key.channel_id.clone(),
            category: "conversation".into(),
            schema_version: 1,
            author: author.into(),
            source: source.into(),
            // Phase 1b: plaintext bytes. Phase 2: vault.encrypt(content, &nonce)
            content_encrypted: content.as_bytes().to_vec(),
            nonce,
            content_hash: None,
            parent_id: None,
            trust_score: if author == "user" { 1.0 } else { 0.8 },
            created_at: chrono::Utc::now(),
            expires_at: None,
        };
        self.store.insert_memory(&entry).await?;
        Ok(())
    }

    /// Get recent conversation entries.
    pub async fn recent(
        &self,
        key: &SessionKey,
        limit: usize,
    ) -> Result<Vec<springtale_store::MemoryRow>, BotError> {
        let entries = self
            .store
            .get_memory(&key.user_id, &key.channel_id, limit)
            .await?;
        Ok(entries)
    }

    /// Compact the conversation context, keeping only the newest entries.
    pub async fn compact(&self, key: &SessionKey) -> Result<u64, BotError> {
        let deleted = self
            .store
            .compact_memory(&key.user_id, &key.channel_id, self.max_messages)
            .await?;
        Ok(deleted)
    }
}
