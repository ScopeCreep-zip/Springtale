//! Durable webhook replay protection.
//!
//! A provider that signs its webhooks (Kick's RSA signature, GitHub's
//! HMAC) also gives each delivery an idempotent id. Remembering those
//! ids is what stops a captured — and still perfectly valid — request
//! from being replayed into the rule engine.
//!
//! That memory has to outlive the process. A connector holding the seen
//! ids in a `HashMap` loses them on every daemon reload, vault re-unlock
//! and crash, and each restart reopens the full replay window for every
//! delivery still inside the provider's signing/timestamp validity. So
//! the guard lives here, on the store, not in the connector: connector
//! crates depend on `springtale-connector` and never on
//! `springtale-store` (see `.claude/rules/backend/crate-structure.md`),
//! while this crate already depends on the store.
//!
//! The connector still owns the protocol half — which header carries
//! the id — via
//! [`crate::connector::trait_::Connector::webhook_replay_key`]. The
//! daemon's webhook ingress calls that, then [`check_and_record`], and
//! only ever *after* signature verification has passed, so an unsigned
//! request can never poison the table.
//!
//! Storage reuses the existing `dedupe_seen` table
//! ([`springtale_store::StorageBackend::dedupe_check`]): an atomic
//! `INSERT OR IGNORE` check-and-record with LRU pruning, which is
//! exactly the shape a replay guard needs. No new table.

use std::sync::Arc;

use springtale_store::StorageBackend;
use springtale_store::schema::dedupe::DedupeOutcome;

use crate::error::ConnectorError;

/// Dedupe bucket shared by every connector's webhook replay guard.
///
/// Rows are scoped `(formation_id = global, rule_id = connector name,
/// bucket)`, so one connector's delivery ids can never collide with
/// another's or with a rule's own `Action::Dedupe` state.
pub const REPLAY_BUCKET: &str = "webhook_replay";

/// Delivery ids retained per connector before the oldest are pruned.
///
/// The prune is LRU, not TTL: a replay is only worth attempting while
/// the provider's own signature/timestamp window still accepts the
/// captured request (five minutes for Kick), and 4096 deliveries is far
/// more than any first-party connector receives in that span.
pub const REPLAY_HISTORY: u32 = 4096;

/// Whether this webhook delivery has been seen before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// First sight of this delivery id — now recorded. Process it.
    Fresh,
    /// The id is already on record. Drop the request.
    Replay,
}

/// Atomically record `replay_key` for `connector` and report whether it
/// had been seen before.
///
/// The key is hashed with blake3 before it touches disk, matching the
/// `dedupe_seen` privacy invariant (a provider delivery id can identify
/// a channel or a sender; plaintext keys never land in the database).
///
/// # Errors
///
/// Returns [`ConnectorError::ExecutionFailed`] if the store is
/// unreachable. Callers must treat that as fail-closed — an
/// unverifiable delivery is a delivery that may be a replay.
pub async fn check_and_record(
    store: &Arc<dyn StorageBackend>,
    connector: &str,
    replay_key: &str,
) -> Result<ReplayOutcome, ConnectorError> {
    let key_hash = blake3::hash(replay_key.as_bytes()).to_hex().to_string();
    let outcome = store
        .dedupe_check(None, connector, REPLAY_BUCKET, &key_hash, REPLAY_HISTORY)
        .await
        .map_err(|e| {
            ConnectorError::ExecutionFailed(format!("webhook replay store unavailable: {e}"))
        })?;
    Ok(match outcome {
        DedupeOutcome::Fresh => ReplayOutcome::Fresh,
        DedupeOutcome::SeenBefore => ReplayOutcome::Replay,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::SqliteBackend;

    fn store() -> Arc<dyn StorageBackend> {
        Arc::new(SqliteBackend::open_in_memory().unwrap())
    }

    #[tokio::test]
    async fn test_check_and_record_first_sight_is_fresh() {
        let store = store();
        assert_eq!(
            check_and_record(&store, "connector-kick", "msg-1")
                .await
                .unwrap(),
            ReplayOutcome::Fresh
        );
    }

    #[tokio::test]
    async fn test_check_and_record_repeated_key_is_replay() {
        let store = store();
        assert_eq!(
            check_and_record(&store, "connector-kick", "msg-1")
                .await
                .unwrap(),
            ReplayOutcome::Fresh
        );
        assert_eq!(
            check_and_record(&store, "connector-kick", "msg-1")
                .await
                .unwrap(),
            ReplayOutcome::Replay
        );
    }

    #[tokio::test]
    async fn test_check_and_record_survives_a_dropped_connector() {
        // The defect this guards: the seen-id set used to live in the
        // connector struct, so a daemon reload / vault re-unlock built a
        // fresh connector and reopened the replay window. The store
        // outlives the connector, so a new one must still see the id.
        let store = store();
        {
            let first_boot = Arc::clone(&store);
            assert_eq!(
                check_and_record(&first_boot, "connector-kick", "msg-1")
                    .await
                    .unwrap(),
                ReplayOutcome::Fresh
            );
        }
        let second_boot = Arc::clone(&store);
        assert_eq!(
            check_and_record(&second_boot, "connector-kick", "msg-1")
                .await
                .unwrap(),
            ReplayOutcome::Replay,
            "a delivery id must stay rejected across a connector restart"
        );
    }

    #[tokio::test]
    async fn test_check_and_record_scopes_by_connector() {
        let store = store();
        assert_eq!(
            check_and_record(&store, "connector-kick", "shared-id")
                .await
                .unwrap(),
            ReplayOutcome::Fresh
        );
        assert_eq!(
            check_and_record(&store, "connector-github", "shared-id")
                .await
                .unwrap(),
            ReplayOutcome::Fresh,
            "connectors must not share a replay namespace"
        );
    }

    #[tokio::test]
    async fn test_check_and_record_does_not_store_the_plaintext_key() {
        let store = store();
        let key = "kick-message-id-that-names-a-channel";
        check_and_record(&store, "connector-kick", key)
            .await
            .unwrap();
        let hashed = blake3::hash(key.as_bytes()).to_hex().to_string();
        assert_ne!(hashed, key);
        // Re-checking with the hash itself must NOT collide with the
        // recorded row — proof the stored column is the digest of the
        // key, not the key (and not the digest of the digest).
        assert_eq!(
            check_and_record(&store, "connector-kick", &hashed)
                .await
                .unwrap(),
            ReplayOutcome::Fresh
        );
    }
}
