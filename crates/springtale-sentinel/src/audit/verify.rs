//! Audit-log row-hash chain verifier (Phase-7 audit Finding B).
//!
//! Walks every row in `audit_trail` in `chain_seq` ascending order,
//! recomputes `row_hash = SHA-256(prev_hash || canonical_row_json)`
//! per row, and compares against the persisted `row_hash`. Any
//! mismatch — a mutated field, a reordered row, a deleted row that
//! breaks the link to the next — fails the check.
//!
//! The genesis anchor (`prev_hash` of the first row) is the SHA-256
//! hex of the vault identity key's public bytes, so a fresh SQLite
//! with the same vault picks up where the previous chain left off; a
//! fresh SQLite + different vault starts a new chain. The verifier
//! therefore takes the expected anchor as an argument — if the first
//! row's persisted `prev_hash` doesn't match, the chain is broken.

use std::sync::Arc;

use springtale_store::StorageBackend;
use springtale_store::schema::audit_chain::compute_row_hash;
use thiserror::Error;

/// Result of a successful walk — the number of rows verified and the
/// `row_hash` of the chain tip (handy for forensic snapshots).
#[derive(Debug, Clone)]
pub struct ChainOk {
    pub rows_verified: u64,
    pub tip_hash: String,
}

/// Per-row chain break with enough context to reconstruct what went
/// wrong. `expected` is what the verifier computed locally; `observed`
/// is what the row carries on disk.
#[derive(Debug, Clone, Error)]
#[error(
    "audit chain broken at chain_seq={chain_seq} (row id {row_id}): \
     expected {expected}, observed {observed}"
)]
pub struct ChainBroken {
    pub row_id: uuid::Uuid,
    pub chain_seq: i64,
    pub expected: String,
    pub observed: String,
    pub reason: ChainBreakReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainBreakReason {
    /// First row's `prev_hash` does not equal the expected genesis
    /// anchor. Means either the chain belongs to a different vault or
    /// the first row was tampered with.
    GenesisMismatch,
    /// A row's `prev_hash` does not equal the previous row's
    /// `row_hash`. Means a row was deleted or reordered.
    PrevHashMismatch,
    /// A row's recomputed `row_hash` does not match the stored value.
    /// Means a field on that row was tampered with after INSERT.
    RowHashMismatch,
    /// `chain_seq` is not strictly increasing by 1. Means a row was
    /// deleted or a duplicate slipped in.
    ChainSeqGap,
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("audit chain broken: {0}")]
    ChainBroken(#[from] ChainBroken),
    #[error("store error: {0}")]
    Store(#[from] springtale_store::StoreError),
}

/// Walk the chain and verify each row. `genesis_anchor` is the
/// expected `prev_hash` of the first row (typically
/// `vault_genesis_anchor(vault_pub_bytes)` from
/// `springtale_store::schema::audit_chain`). Returns the rows
/// verified + the chain tip hash on success.
///
/// On an empty audit trail returns `ChainOk { rows_verified: 0,
/// tip_hash: genesis_anchor.to_owned() }` — the empty chain is
/// always valid.
pub async fn verify_chain(
    store: &Arc<dyn StorageBackend>,
    genesis_anchor: &str,
) -> Result<ChainOk, VerifyError> {
    let rows = store.list_audit_chain().await?;
    if rows.is_empty() {
        return Ok(ChainOk {
            rows_verified: 0,
            tip_hash: genesis_anchor.to_owned(),
        });
    }

    let mut expected_prev = genesis_anchor.to_owned();
    let mut expected_seq: i64 = 1;
    let mut last_hash = String::new();

    for row in &rows {
        // chain_seq must be strictly monotonic +1.
        if row.chain_seq != expected_seq {
            return Err(VerifyError::ChainBroken(ChainBroken {
                row_id: row.id,
                chain_seq: row.chain_seq,
                expected: expected_seq.to_string(),
                observed: row.chain_seq.to_string(),
                reason: ChainBreakReason::ChainSeqGap,
            }));
        }

        // prev_hash must link to the previous row's row_hash (or to
        // the genesis anchor for chain_seq == 1).
        if row.prev_hash != expected_prev {
            let reason = if row.chain_seq == 1 {
                ChainBreakReason::GenesisMismatch
            } else {
                ChainBreakReason::PrevHashMismatch
            };
            return Err(VerifyError::ChainBroken(ChainBroken {
                row_id: row.id,
                chain_seq: row.chain_seq,
                expected: expected_prev.clone(),
                observed: row.prev_hash.clone(),
                reason,
            }));
        }

        // row_hash must match the recomputed hash for the row's
        // canonical content.
        let recomputed = compute_row_hash(&row.prev_hash, row);
        if recomputed != row.row_hash {
            return Err(VerifyError::ChainBroken(ChainBroken {
                row_id: row.id,
                chain_seq: row.chain_seq,
                expected: recomputed,
                observed: row.row_hash.clone(),
                reason: ChainBreakReason::RowHashMismatch,
            }));
        }

        expected_prev = row.row_hash.clone();
        expected_seq += 1;
        last_hash = row.row_hash.clone();
    }

    // Every row reaching here passed; a failed row returns Err above.
    Ok(ChainOk {
        rows_verified: rows.len() as u64,
        tip_hash: last_hash,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::backend::InMemoryBackend;
    use springtale_store::schema::audit::AuditEntry as Entry;

    fn entry(connector: &str) -> Entry {
        Entry {
            id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            connector_name: connector.into(),
            action_type: "RunConnector".into(),
            action_summary: "ping".into(),
            verdict: "go".into(),
            verdict_reason: String::new(),
            result: "ok".into(),
            prev_hash: String::new(),
            row_hash: String::new(),
            chain_seq: 0,
        }
    }

    #[tokio::test]
    async fn empty_chain_verifies() {
        let store: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let ok = verify_chain(&store, "anchor").await.unwrap();
        assert_eq!(ok.rows_verified, 0);
        assert_eq!(ok.tip_hash, "anchor");
    }

    #[tokio::test]
    async fn three_row_chain_verifies() {
        let store: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        for name in ["a", "b", "c"] {
            store.insert_audit_entry(&entry(name)).await.unwrap();
        }
        let ok = verify_chain(&store, "").await.unwrap();
        assert_eq!(ok.rows_verified, 3);
        assert_eq!(ok.tip_hash.len(), 64);
    }

    #[tokio::test]
    async fn genesis_mismatch_breaks_chain() {
        let store: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        store.insert_audit_entry(&entry("a")).await.unwrap();
        // Insert path stamps prev_hash = "" for the first row; if we
        // expect "another-anchor" instead we should detect the
        // mismatch.
        let err = verify_chain(&store, "another-anchor").await.unwrap_err();
        match err {
            VerifyError::ChainBroken(b) => {
                assert_eq!(b.reason, ChainBreakReason::GenesisMismatch);
            }
            _ => panic!("expected ChainBroken"),
        }
    }
}
