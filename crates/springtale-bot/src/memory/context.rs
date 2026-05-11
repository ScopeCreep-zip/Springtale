use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::BotError;
use crate::memory::persistent::{MemoryStore, MemoryWrite};
use crate::state::session::SessionKey;

/// Sliding window of recent conversation per (user_id, channel_id).
///
/// Persistence + encryption live in [`MemoryStore`]; this type owns
/// the window-size policy, AI-summarization-on-overflow, and the
/// session-key boundary. Earlier revisions inlined the
/// encrypt/decrypt calls here; the bug that triggered the rewrite
/// was that the AI-summary path then forgot to encrypt the summary
/// row before persisting it, leaking plaintext on disk while the
/// regular `push` path encrypted correctly. Centralising both
/// writes through `MemoryStore` eliminates that whole class of
/// asymmetry: every write goes through one encrypting boundary.
pub struct ConversationContext {
    max_messages: usize,
    store: Arc<dyn StorageBackend>,
    ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
    memory: MemoryStore,
}

impl ConversationContext {
    pub fn new(
        store: Arc<dyn StorageBackend>,
        max_messages: usize,
        ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
        encryption_key: [u8; 32],
    ) -> Self {
        let memory = MemoryStore::new(store.clone(), encryption_key);
        Self {
            max_messages,
            store,
            ai_adapter,
            memory,
        }
    }

    /// Push a new message into the conversation context. Plaintext
    /// is encrypted before persistence by [`MemoryStore::store`].
    pub async fn push(
        &self,
        key: &SessionKey,
        author: &str,
        content: &str,
    ) -> Result<(), BotError> {
        let source = match author {
            "user" => "user_input",
            "assistant" => "connector_output",
            _ => "connector_output",
        };
        self.memory
            .store(MemoryWrite {
                user_id: &key.user_id,
                channel_id: &key.channel_id,
                category: "conversation",
                author,
                source,
                content,
                trust_score: if author == "user" { 1.0 } else { 0.8 },
                parent_id: None,
                expires_at: None,
            })
            .await?;
        Ok(())
    }

    /// Get recent conversation entries, decrypted in place by
    /// [`MemoryStore::recall`]. `content_encrypted` on each
    /// returned row carries the decrypted plaintext bytes.
    pub async fn recent(
        &self,
        key: &SessionKey,
        limit: usize,
    ) -> Result<Vec<springtale_store::MemoryRow>, BotError> {
        self.memory
            .recall(&key.user_id, &key.channel_id, limit)
            .await
    }

    /// Compact the conversation context, keeping only the newest entries.
    ///
    /// If AI is available, summarizes the oldest entries into a single
    /// summary entry (source="compacted", trust_score=0.5 per spec §15.5)
    /// before deleting them. Falls back to simple truncation if AI is
    /// unavailable or errors.
    pub async fn compact(&self, key: &SessionKey) -> Result<u64, BotError> {
        // Pull *decrypted* entries via the memory store so the
        // summarization prompt sees plaintext, not ciphertext. Read
        // every entry — compaction can't decide what to drop without
        // seeing all of them.
        let all_plaintext = self
            .memory
            .recall(&key.user_id, &key.channel_id, usize::MAX)
            .await?;

        if all_plaintext.len() <= self.max_messages {
            return Ok(0);
        }

        if self.ai_adapter.is_available().await {
            match self
                .ai_summarize(key, &all_plaintext, self.max_messages)
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

    /// Summarize oldest entries via AI and replace them with a
    /// single summary entry. `all_entries` carries *plaintext* in
    /// the `content_encrypted` field (per `MemoryStore::recall`);
    /// the summary is persisted through `MemoryStore::store` so it
    /// is encrypted on disk like every other entry — earlier the
    /// summary row leaked plaintext to the database because it was
    /// inserted directly via `store.insert_memory` without going
    /// through the encrypt path.
    async fn ai_summarize(
        &self,
        key: &SessionKey,
        all_entries: &[springtale_store::MemoryRow],
        max_entries: usize,
    ) -> Result<u64, BotError> {
        let to_summarize = all_entries.len() - max_entries;
        // Oldest entries are at the END (recall preserves the
        // backend's DESC order).
        let oldest = &all_entries[all_entries.len() - to_summarize..];

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

        let oldest_id = oldest.last().map(|e| e.id.clone());

        self.memory
            .store(MemoryWrite {
                user_id: &key.user_id,
                channel_id: &key.channel_id,
                category: "conversation",
                author: "agent",       // per spec MemoryAuthor::Agent
                source: "compacted",   // per spec MemorySource::Compacted
                content: &response.content,
                trust_score: 0.5, // per spec §15.5: AI-generated = 0.5
                parent_id: oldest_id, // compaction chain tracking
                expires_at: None,
            })
            .await?;

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
