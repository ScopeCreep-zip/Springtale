use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::BotError;
use crate::state::session::SessionKey;

/// Sliding window of recent conversation per (user_id, channel_id).
pub struct ConversationContext {
    max_messages: usize,
    store: Arc<dyn StorageBackend>,
    ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
}

impl ConversationContext {
    pub fn new(
        store: Arc<dyn StorageBackend>,
        max_messages: usize,
        ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
    ) -> Self {
        Self {
            max_messages,
            store,
            ai_adapter,
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
    ///
    /// If AI is available, summarizes the oldest entries into a single
    /// summary entry (source="compacted", trust_score=0.5 per spec §15.5)
    /// before deleting them. Falls back to simple truncation if AI is
    /// unavailable or errors.
    pub async fn compact(&self, key: &SessionKey) -> Result<u64, BotError> {
        // Get all entries to check count
        let all_entries = self
            .store
            .get_memory(&key.user_id, &key.channel_id, usize::MAX)
            .await?;

        if all_entries.len() <= self.max_messages {
            return Ok(0);
        }

        // Try AI summarization if available
        if self.ai_adapter.is_available().await {
            match self
                .ai_summarize(key, &all_entries, self.max_messages)
                .await
            {
                Ok(deleted) => return Ok(deleted),
                Err(e) => {
                    tracing::warn!(error = %e, "AI summarization failed — falling back to truncation");
                }
            }
        }

        // Fallback: simple truncation
        let deleted = self
            .store
            .compact_memory(&key.user_id, &key.channel_id, self.max_messages)
            .await?;
        Ok(deleted)
    }

    /// Summarize oldest entries via AI and replace them with a summary entry.
    async fn ai_summarize(
        &self,
        key: &SessionKey,
        all_entries: &[springtale_store::MemoryRow],
        max_entries: usize,
    ) -> Result<u64, BotError> {
        let to_summarize = all_entries.len() - max_entries;
        // Oldest entries are at the END (get_memory returns DESC order)
        let oldest = &all_entries[all_entries.len() - to_summarize..];

        // Build summarization prompt
        let mut conversation = String::new();
        for entry in oldest.iter().rev() {
            let content = String::from_utf8_lossy(&entry.content_encrypted);
            conversation.push_str(&format!("[{}]: {}\n", entry.author, content));
        }

        let prompt = format!(
            "Summarize the following conversation into a concise paragraph \
             preserving key facts, decisions, and important context:\n\n{conversation}"
        );

        let request = springtale_ai::AiRequest::Complete { prompt };
        let options = springtale_ai::AiOptions {
            max_tokens: 256,
            timeout: std::time::Duration::from_secs(15),
            temperature: Some(0.3),
        };

        let response = self.ai_adapter.complete(request, options).await?;

        if response.content.is_empty() {
            return Err(BotError::Memory("AI returned empty summary".into()));
        }

        // Insert summary entry with correct provenance per spec §15.5
        let nonce = {
            use rand::RngCore;
            let mut n = vec![0u8; 24];
            rand::thread_rng().fill_bytes(&mut n);
            n
        };

        let oldest_id = oldest.last().map(|e| e.id.clone());
        let oldest_timestamp = oldest
            .last()
            .map(|e| e.created_at)
            .unwrap_or_else(chrono::Utc::now);

        let summary_entry = springtale_store::MemoryRow {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: key.user_id.clone(),
            channel_id: key.channel_id.clone(),
            category: "conversation".into(),
            schema_version: 1,
            author: "agent".into(),     // per spec MemoryAuthor::Agent
            source: "compacted".into(), // per spec MemorySource::Compacted
            content_encrypted: response.content.as_bytes().to_vec(),
            nonce,
            content_hash: None,
            parent_id: oldest_id, // compaction chain tracking
            trust_score: 0.5,     // per spec §15.5: AI-generated = 0.5
            created_at: oldest_timestamp,
            expires_at: None,
        };

        self.store.insert_memory(&summary_entry).await?;

        // Delete the original oldest entries
        // Use compact_memory which keeps the newest max_entries
        let deleted = self
            .store
            .compact_memory(&key.user_id, &key.channel_id, max_entries)
            .await?;

        tracing::info!(
            summarized = to_summarize,
            deleted = deleted,
            "AI summarization compaction complete"
        );

        Ok(deleted)
    }
}
