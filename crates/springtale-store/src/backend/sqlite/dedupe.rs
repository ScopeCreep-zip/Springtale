//! SQLite-backed dedupe — atomic check-and-record for
//! `Action::Dedupe` chain steps.
//!
//! Per `feedback_no_redundant_tests` and the privacy invariant in
//! CLAUDE.md §6.10, the key is always pre-hashed by the runtime
//! (`blake3`) before reaching this layer. We store the hex digest as
//! TEXT, never plaintext keys.

use chrono::Utc;
use rusqlite::params;

use crate::error::StoreError;
use crate::schema::dedupe::DedupeOutcome;

use super::SqliteBackend;

/// Empty-string sentinel for "global rule, no formation". SQLite
/// STRICT mode forbids NULL in primary-key columns, and even outside
/// STRICT mode SQLite treats NULL values as distinct from each other
/// in UNIQUE constraints (so two `NULL` formation_ids wouldn't
/// collide). Mapping `None → ""` at the boundary preserves "global"
/// scoping semantics while keeping the PK a single column tuple.
const GLOBAL_FORMATION_SENTINEL: &str = "";

impl SqliteBackend {
    pub(super) async fn dedupe_check_impl(
        &self,
        formation_id: Option<String>,
        rule_id: String,
        bucket: String,
        key_hash: String,
        history: u32,
    ) -> Result<DedupeOutcome, StoreError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| StoreError::Database("lock poisoned".into()))?;

            let now_ms: i64 = Utc::now().timestamp_millis();
            let formation_key: String =
                formation_id.unwrap_or_else(|| GLOBAL_FORMATION_SENTINEL.to_owned());

            // Atomic check-and-record via INSERT OR IGNORE — if the
            // PK already exists, the insert is a no-op (0 rows
            // changed) and we report SeenBefore. Otherwise it
            // inserts and we report Fresh, then prune to `history`.
            let tx = conn.unchecked_transaction()?;
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO dedupe_seen \
                 (formation_id, rule_id, bucket, key_hash, seen_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![&formation_key, &rule_id, &bucket, &key_hash, now_ms],
            )?;

            if inserted == 0 {
                tx.commit()?;
                return Ok(DedupeOutcome::SeenBefore);
            }

            // Prune oldest if over history threshold. Scope to this
            // bucket only — other buckets' state stays untouched.
            let history = history as i64;
            let kept: i64 = tx.query_row(
                "SELECT COUNT(*) FROM dedupe_seen \
                 WHERE formation_id = ?1 AND rule_id = ?2 AND bucket = ?3",
                params![&formation_key, &rule_id, &bucket],
                |row| row.get(0),
            )?;

            if kept > history {
                let to_remove = kept - history;
                // Order by `seen_at ASC` (oldest first), then by
                // `rowid ASC` as a tie-breaker. Rapid inserts can
                // collide on the millisecond `seen_at`; rowid is
                // monotonically increasing per insert so it gives
                // stable LRU eviction even when timestamps clash.
                tx.execute(
                    "DELETE FROM dedupe_seen \
                     WHERE rowid IN ( \
                       SELECT rowid FROM dedupe_seen \
                       WHERE formation_id = ?1 AND rule_id = ?2 AND bucket = ?3 \
                       ORDER BY seen_at ASC, rowid ASC LIMIT ?4)",
                    params![&formation_key, &rule_id, &bucket, to_remove],
                )?;
            }

            tx.commit()?;
            Ok(DedupeOutcome::Fresh)
        })
        .await
        .map_err(|e| StoreError::Database(format!("spawn_blocking join: {e}")))?
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::SqliteBackend;
    use crate::backend::trait_::StorageBackend;

    #[tokio::test]
    async fn dedupe_check_fresh_then_seen() {
        let store = SqliteBackend::open_in_memory().unwrap();
        let outcome1 = store
            .dedupe_check(Some("formA"), "rule1", "bucket1", "hash_a", 100)
            .await
            .unwrap();
        assert_eq!(outcome1, DedupeOutcome::Fresh);

        let outcome2 = store
            .dedupe_check(Some("formA"), "rule1", "bucket1", "hash_a", 100)
            .await
            .unwrap();
        assert_eq!(outcome2, DedupeOutcome::SeenBefore);
    }

    #[tokio::test]
    async fn dedupe_check_distinct_keys_are_independent() {
        let store = SqliteBackend::open_in_memory().unwrap();
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "b", "hash_a", 100)
                .await
                .unwrap(),
            DedupeOutcome::Fresh
        );
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "b", "hash_b", 100)
                .await
                .unwrap(),
            DedupeOutcome::Fresh
        );
        // Re-checking either marks it seen.
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "b", "hash_a", 100)
                .await
                .unwrap(),
            DedupeOutcome::SeenBefore
        );
    }

    #[tokio::test]
    async fn dedupe_check_distinct_buckets_are_independent() {
        let store = SqliteBackend::open_in_memory().unwrap();
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "bucketA", "h", 100)
                .await
                .unwrap(),
            DedupeOutcome::Fresh
        );
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "bucketB", "h", 100)
                .await
                .unwrap(),
            DedupeOutcome::Fresh
        );
    }

    #[tokio::test]
    async fn dedupe_check_distinct_formations_are_independent() {
        let store = SqliteBackend::open_in_memory().unwrap();
        // Same rule, bucket, key — different formations don't collide.
        assert_eq!(
            store
                .dedupe_check(Some("formA"), "r", "b", "h", 100)
                .await
                .unwrap(),
            DedupeOutcome::Fresh
        );
        assert_eq!(
            store
                .dedupe_check(Some("formB"), "r", "b", "h", 100)
                .await
                .unwrap(),
            DedupeOutcome::Fresh
        );
        // Global rule (None) is its own scope too.
        assert_eq!(
            store.dedupe_check(None, "r", "b", "h", 100).await.unwrap(),
            DedupeOutcome::Fresh
        );
    }

    #[tokio::test]
    async fn dedupe_lru_prunes_oldest_at_history_threshold() {
        let store = SqliteBackend::open_in_memory().unwrap();
        // Insert 5 distinct keys with history = 3.
        for i in 0..5 {
            store
                .dedupe_check(Some("f"), "r", "b", &format!("h{i}"), 3)
                .await
                .unwrap();
        }
        // h0 and h1 should be the oldest two — pruned to keep
        // history = 3 (h2, h3, h4 remain).
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "b", "h0", 3)
                .await
                .unwrap(),
            DedupeOutcome::Fresh,
            "h0 should have been LRU-pruned and so re-inserts as Fresh"
        );
        assert_eq!(
            store
                .dedupe_check(Some("f"), "r", "b", "h4", 3)
                .await
                .unwrap(),
            DedupeOutcome::SeenBefore,
            "h4 is the most recent and should still be present"
        );
    }
}
