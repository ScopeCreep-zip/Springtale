use std::sync::Arc;

use springtale_store::StorageBackend;

use crate::error::BotError;

/// Encrypted memory store backed by SQLite + vault.
///
/// Phase 1b: stores content directly (vault encryption deferred to
/// integration with springtale-crypto vault for per-entry encryption).
/// The schema supports encrypted content; the runtime will wire in
/// vault encryption when the bot identity module provides the key.
pub struct MemoryStore {
    store: Arc<dyn StorageBackend>,
}

impl MemoryStore {
    pub fn new(store: Arc<dyn StorageBackend>) -> Self {
        Self { store }
    }

    /// Store a memory entry.
    pub async fn store(&self, entry: &springtale_store::MemoryRow) -> Result<(), BotError> {
        self.store.insert_memory(entry).await?;
        Ok(())
    }

    /// Recall recent memory entries for a user/channel.
    pub async fn recall(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<springtale_store::MemoryRow>, BotError> {
        let entries = self.store.get_memory(user_id, channel_id, limit).await?;
        Ok(entries)
    }

    /// Forget all memory entries for a user/channel.
    pub async fn forget(&self, user_id: &str, channel_id: &str) -> Result<u64, BotError> {
        let deleted = self.store.delete_memory(user_id, channel_id).await?;
        Ok(deleted)
    }
}
