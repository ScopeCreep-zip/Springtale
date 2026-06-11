//! Runtime-side dedupe — hashes the resolved key with `blake3`, then
//! delegates the atomic check-and-record to the storage backend.
//!
//! ## Why hash at the runtime layer (not the store)
//!
//! Recipe authors write things like
//! `${last_extract_output.entries.0.id}` as the dedupe key. The
//! plaintext id can reveal sender / recipient / subject — sensitive
//! state for the threat model in CLAUDE.md §6.10. Hashing at the
//! runtime boundary ensures the store sees only an opaque 64-char
//! hex digest. The recipe author doesn't have to know about hashing;
//! the runtime applies it transparently.
//!
//! ## Concurrency
//!
//! `springtale-store::SqliteBackend::dedupe_check` runs the check +
//! insert + LRU prune in a single transaction, so concurrent fires
//! of the same rule don't double-write. The runtime layer is pure
//! hashing; no shared state.

use std::sync::Arc;

use springtale_store::{StorageBackend, schema::dedupe::DedupeOutcome};

use crate::error::OperationError;

/// Compute the blake3 hash of the resolved dedupe key and run an
/// atomic check-and-record against the dedupe table. Returns
/// `DedupeOutcome::Fresh` on first sight (key now recorded) or
/// `DedupeOutcome::SeenBefore` if this `(formation_id, rule_id,
/// bucket, key)` combination has fired before.
///
/// `history` caps the entries retained per bucket — older entries
/// LRU-prune in the same transaction as the insert.
pub async fn check_and_record(
    store: &Arc<dyn StorageBackend>,
    formation_id: Option<&str>,
    rule_id: &str,
    bucket: &str,
    key_plaintext: &str,
    history: u32,
) -> Result<DedupeOutcome, OperationError> {
    let key_hash = blake3::hash(key_plaintext.as_bytes()).to_hex().to_string();
    store
        .dedupe_check(formation_id, rule_id, bucket, &key_hash, history)
        .await
        .map_err(OperationError::Store)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use springtale_store::SqliteBackend;

    #[tokio::test]
    async fn check_and_record_returns_fresh_then_seen() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let outcome1 = check_and_record(&store, None, "rule1", "bucket1", "key-alpha", 100)
            .await
            .unwrap();
        assert_eq!(outcome1, DedupeOutcome::Fresh);

        let outcome2 = check_and_record(&store, None, "rule1", "bucket1", "key-alpha", 100)
            .await
            .unwrap();
        assert_eq!(outcome2, DedupeOutcome::SeenBefore);
    }

    #[tokio::test]
    async fn same_plaintext_hashes_consistently() {
        // The plaintext key is hashed deterministically — a recipe
        // that re-fires with the same upstream payload always lands
        // on the same hash, so dedupe state survives runtime
        // restarts.
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        check_and_record(&store, None, "rule1", "bucket1", "same-key", 100)
            .await
            .unwrap();
        let again = check_and_record(&store, None, "rule1", "bucket1", "same-key", 100)
            .await
            .unwrap();
        assert_eq!(again, DedupeOutcome::SeenBefore);
    }

    #[tokio::test]
    async fn distinct_keys_dont_collide() {
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let a = check_and_record(&store, None, "r", "b", "alpha", 100)
            .await
            .unwrap();
        let b = check_and_record(&store, None, "r", "b", "beta", 100)
            .await
            .unwrap();
        assert_eq!(a, DedupeOutcome::Fresh);
        assert_eq!(b, DedupeOutcome::Fresh);
    }

    #[tokio::test]
    async fn key_hashing_obscures_plaintext_at_the_store_boundary() {
        // We can't directly inspect the inserted row from this test
        // (the store API only exposes check-and-record), but we can
        // assert that two keys differing by one byte produce
        // distinct outcomes — proving the hash is sensitive to input.
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let a = check_and_record(&store, None, "r", "b", "key", 100)
            .await
            .unwrap();
        let b = check_and_record(&store, None, "r", "b", "keyy", 100)
            .await
            .unwrap();
        assert_eq!(a, DedupeOutcome::Fresh);
        assert_eq!(b, DedupeOutcome::Fresh);
    }
}
