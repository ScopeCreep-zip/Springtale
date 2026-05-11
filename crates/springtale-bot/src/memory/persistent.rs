use std::sync::Arc;

use rand::RngCore;
use springtale_store::StorageBackend;

use crate::error::BotError;

/// Encrypted persistence layer for bot memory entries.
///
/// Owns the `[u8; 32]` XChaCha20-Poly1305 key derived from the vault
/// at bot init; every `store` call encrypts the plaintext with a
/// fresh random 24-byte nonce before writing, and every `recall`
/// call decrypts in place before returning rows. Callers therefore
/// never touch ciphertext — the encryption boundary lives entirely
/// here, matching the project rule "all business logic in named
/// modules" and giving the audit trail a single seam to verify.
///
/// `MemoryRow.content_encrypted` carries the ciphertext bytes on
/// disk; `recall` overwrites that field with the decrypted plaintext
/// (UTF-8 bytes) before returning, so consumers can lift `content`
/// out as `String::from_utf8(row.content_encrypted)` per the same
/// pattern `ConversationContext` already uses.
pub struct MemoryStore {
    store: Arc<dyn StorageBackend>,
    encryption_key: [u8; 32],
}

/// Plaintext input for [`MemoryStore::store`]. Avoids forcing
/// callers to fill in seven optional row fields the persistence
/// layer can default sensibly.
pub struct MemoryWrite<'a> {
    pub user_id: &'a str,
    pub channel_id: &'a str,
    pub category: &'a str,
    pub author: &'a str,
    pub source: &'a str,
    pub content: &'a str,
    pub trust_score: f64,
    pub parent_id: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl MemoryStore {
    /// Construct with a vault-derived 32-byte key.
    pub fn new(store: Arc<dyn StorageBackend>, encryption_key: [u8; 32]) -> Self {
        Self {
            store,
            encryption_key,
        }
    }

    /// Encrypt + persist a memory entry.
    pub async fn store(&self, write: MemoryWrite<'_>) -> Result<String, BotError> {
        let mut nonce = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce);

        let ciphertext = springtale_crypto::message::encrypt_message(
            write.content.as_bytes(),
            &nonce,
            &self.encryption_key,
        )
        .map_err(|e| BotError::Memory(format!("encryption failed: {e}")))?;

        let id = uuid::Uuid::new_v4().to_string();
        let row = springtale_store::MemoryRow {
            id: id.clone(),
            user_id: write.user_id.to_owned(),
            channel_id: write.channel_id.to_owned(),
            category: write.category.to_owned(),
            schema_version: 1,
            author: write.author.to_owned(),
            source: write.source.to_owned(),
            content_encrypted: ciphertext,
            nonce: nonce.to_vec(),
            content_hash: None,
            parent_id: write.parent_id,
            trust_score: write.trust_score,
            created_at: chrono::Utc::now(),
            expires_at: write.expires_at,
        };
        self.store.insert_memory(&row).await?;
        Ok(id)
    }

    /// Persist a pre-built [`springtale_store::MemoryRow`] whose
    /// `content_encrypted` is **plaintext**; the row is mutated in
    /// place to encrypt before insert. Used by paths that already
    /// assemble a row (compaction summaries, migration fixtures).
    pub async fn store_row(
        &self,
        mut row: springtale_store::MemoryRow,
    ) -> Result<(), BotError> {
        let mut nonce = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ciphertext = springtale_crypto::message::encrypt_message(
            &row.content_encrypted,
            &nonce,
            &self.encryption_key,
        )
        .map_err(|e| BotError::Memory(format!("encryption failed: {e}")))?;
        row.content_encrypted = ciphertext;
        row.nonce = nonce.to_vec();
        self.store.insert_memory(&row).await?;
        Ok(())
    }

    /// Recall and decrypt recent entries. `content_encrypted` on
    /// each returned row is overwritten with the decrypted plaintext
    /// (UTF-8 bytes) so callers can treat the field uniformly.
    pub async fn recall(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<springtale_store::MemoryRow>, BotError> {
        let mut entries = self.store.get_memory(user_id, channel_id, limit).await?;
        for entry in &mut entries {
            let nonce: [u8; 24] = entry
                .nonce
                .as_slice()
                .try_into()
                .map_err(|_| BotError::Memory("invalid nonce length".into()))?;
            let plaintext = springtale_crypto::message::decrypt_message(
                &entry.content_encrypted,
                &nonce,
                &self.encryption_key,
            )
            .map_err(|e| BotError::Memory(format!("decryption failed: {e}")))?;
            entry.content_encrypted = plaintext;
        }
        Ok(entries)
    }

    /// Forget all entries for a user/channel pair.
    pub async fn forget(&self, user_id: &str, channel_id: &str) -> Result<u64, BotError> {
        let deleted = self.store.delete_memory(user_id, channel_id).await?;
        Ok(deleted)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::SqliteBackend;

    fn test_store() -> MemoryStore {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        MemoryStore::new(store, [42u8; 32])
    }

    #[tokio::test]
    async fn round_trip_plaintext() {
        let store = test_store();
        store
            .store(MemoryWrite {
                user_id: "u1",
                channel_id: "c1",
                category: "conversation",
                author: "user",
                source: "user_input",
                content: "the secret is 42",
                trust_score: 1.0,
                parent_id: None,
                expires_at: None,
            })
            .await
            .unwrap();

        let rows = store.recall("u1", "c1", 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        let plaintext = String::from_utf8(rows[0].content_encrypted.clone()).unwrap();
        assert_eq!(plaintext, "the secret is 42");
    }

    #[tokio::test]
    async fn ciphertext_is_actually_encrypted_on_disk() {
        let store = test_store();
        store
            .store(MemoryWrite {
                user_id: "u1",
                channel_id: "c1",
                category: "conversation",
                author: "user",
                source: "user_input",
                content: "plaintext-marker",
                trust_score: 1.0,
                parent_id: None,
                expires_at: None,
            })
            .await
            .unwrap();
        // Read raw rows via the backend bypassing recall's decrypt.
        let raw = store
            .store
            .get_memory("u1", "c1", 10)
            .await
            .unwrap();
        assert_eq!(raw.len(), 1);
        assert!(
            !raw[0].content_encrypted.windows(16).any(|w| w == b"plaintext-marker"),
            "plaintext leaked into encrypted-at-rest field"
        );
        assert_eq!(raw[0].nonce.len(), 24, "missing per-entry nonce");
    }

    #[tokio::test]
    async fn forget_removes_entries() {
        let store = test_store();
        store
            .store(MemoryWrite {
                user_id: "u1",
                channel_id: "c1",
                category: "conversation",
                author: "user",
                source: "user_input",
                content: "x",
                trust_score: 1.0,
                parent_id: None,
                expires_at: None,
            })
            .await
            .unwrap();
        assert_eq!(store.forget("u1", "c1").await.unwrap(), 1);
        assert_eq!(store.recall("u1", "c1", 10).await.unwrap().len(), 0);
    }
}
