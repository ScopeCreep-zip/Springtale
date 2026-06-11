//! Audit-trail row-hash chain construction (Phase-7 audit Finding B).
//!
//! The chain binds each `audit_trail` row to its predecessor via
//! `row_hash = SHA-256(prev_hash || canonical_row_json)`. A verifier
//! walks the chain in `chain_seq` order on daemon startup and fails
//! closed on any mismatch — tamper-evident audit log.
//!
//! The canonical JSON is the sorted-key serialization of every
//! field EXCEPT the chain columns themselves (`prev_hash`,
//! `row_hash`, `chain_seq`). Sorted keys keep the hash
//! deterministic across SQLite versions and serde-json patch
//! releases.
//!
//! Genesis anchor: `prev_hash` of the first row is the SHA-256 hex
//! of the vault identity key's public bytes — so the chain is bound
//! to the vault, not just the SQLite file. A fresh SQLite + same
//! vault picks up where the previous chain left off; a fresh SQLite
//! + different vault starts a new chain.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::audit::AuditEntry;

/// Compute the `row_hash` for an entry given its `prev_hash`. The
/// caller mutates the entry to set both `prev_hash` and the
/// returned hash before INSERT.
pub fn compute_row_hash(prev_hash: &str, entry: &AuditEntry) -> String {
    let canonical = canonical_row_json(entry);
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

/// Canonicalise an `AuditEntry` for hashing. Excludes the chain
/// columns (so the hash function is self-referential without infinite
/// regress). Keys are sorted via `BTreeMap` iteration order.
fn canonical_row_json(entry: &AuditEntry) -> String {
    let mut m: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    m.insert("id", serde_json::Value::String(entry.id.to_string()));
    m.insert(
        "timestamp",
        serde_json::Value::String(entry.timestamp.to_rfc3339()),
    );
    m.insert(
        "connector_name",
        serde_json::Value::String(entry.connector_name.clone()),
    );
    m.insert(
        "action_type",
        serde_json::Value::String(entry.action_type.clone()),
    );
    m.insert(
        "action_summary",
        serde_json::Value::String(entry.action_summary.clone()),
    );
    m.insert("verdict", serde_json::Value::String(entry.verdict.clone()));
    m.insert(
        "verdict_reason",
        serde_json::Value::String(entry.verdict_reason.clone()),
    );
    m.insert("result", serde_json::Value::String(entry.result.clone()));
    serde_json::to_string(&m).unwrap_or_default()
}

/// Hex-encoded SHA-256 of the vault identity public-key bytes. Used
/// as the genesis anchor (the `prev_hash` of the first row in a
/// fresh chain). Callers obtain the public-key bytes from
/// `springtale_crypto::identity::keypair::Keypair::verifying_key()
/// .to_bytes()` and hand them to this function once at boot.
pub fn vault_genesis_anchor(vault_identity_public_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(vault_identity_public_bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn entry() -> AuditEntry {
        AuditEntry {
            id: Uuid::nil(),
            timestamp: Utc.with_ymd_and_hms(2026, 6, 2, 12, 0, 0).unwrap(),
            connector_name: "connector-test".into(),
            action_type: "RunConnector".into(),
            action_summary: "ping".into(),
            verdict: "go".into(),
            verdict_reason: "".into(),
            result: "ok".into(),
            prev_hash: "".into(),
            row_hash: "".into(),
            chain_seq: 1,
        }
    }

    #[test]
    fn deterministic_hash_for_same_inputs() {
        let e = entry();
        let h1 = compute_row_hash("anchor", &e);
        let h2 = compute_row_hash("anchor", &e);
        assert_eq!(h1, h2, "same inputs must hash the same");
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn different_prev_hash_changes_row_hash() {
        let e = entry();
        let a = compute_row_hash("anchor-a", &e);
        let b = compute_row_hash("anchor-b", &e);
        assert_ne!(a, b);
    }

    #[test]
    fn field_change_changes_hash() {
        let mut e = entry();
        let baseline = compute_row_hash("anchor", &e);
        e.verdict = "throttle".into();
        let mutated = compute_row_hash("anchor", &e);
        assert_ne!(baseline, mutated, "verdict mutation must change row_hash");
    }

    #[test]
    fn chain_column_mutation_does_not_change_hash() {
        // The chain columns are EXCLUDED from canonical_row_json so
        // changing prev_hash / row_hash / chain_seq doesn't affect
        // the computed hash for the row's content.
        let mut e = entry();
        let baseline = compute_row_hash("anchor", &e);
        e.prev_hash = "ignored".into();
        e.row_hash = "ignored".into();
        e.chain_seq = 999;
        let after = compute_row_hash("anchor", &e);
        assert_eq!(baseline, after);
    }

    #[test]
    fn genesis_anchor_is_hex_64() {
        let h = vault_genesis_anchor(b"vault-pub-bytes");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn genesis_anchor_is_deterministic() {
        let a = vault_genesis_anchor(b"vault-pub-bytes");
        let b = vault_genesis_anchor(b"vault-pub-bytes");
        assert_eq!(a, b);
    }
}
