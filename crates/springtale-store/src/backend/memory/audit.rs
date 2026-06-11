use chrono::{DateTime, Utc};

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};
use crate::schema::audit_chain::compute_row_hash;

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn insert_audit_entry_impl(
        &self,
        entry: &AuditEntry,
    ) -> Result<(), StoreError> {
        // Chain hashing mirrors the SQLite path: read the chain tip,
        // compute the new row's hash, store both. The vault genesis
        // anchor (if any) is the previous row's `prev_hash` — for an
        // empty table we fall back to the empty string.
        // Resolve the genesis anchor BEFORE taking the audit write
        // lock so we don't hold two locks at once. The anchor is
        // stored under `audit.chain.anchor` in the generic KV; on
        // first-ever insert (chain empty) it becomes row 1's
        // prev_hash. If unset (tests, pre-migration), we fall back
        // to the empty string.
        let anchor = match self.get_config_impl("audit.chain.anchor").await? {
            Some(raw) => serde_json::from_str::<String>(&raw).unwrap_or(raw),
            None => String::new(),
        };
        let mut audit = self.audit.write().await;
        let (prev_hash, chain_seq) = audit
            .last()
            .map(|last| (last.row_hash.clone(), last.chain_seq + 1))
            .unwrap_or((anchor, 1));
        let mut e = entry.clone();
        e.prev_hash = prev_hash.clone();
        e.chain_seq = chain_seq;
        e.row_hash = compute_row_hash(&prev_hash, &e);
        audit.push(e);
        Ok(())
    }

    pub(super) async fn list_audit_entries_impl(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let audit = self.audit.read().await;
        let filtered: Vec<AuditEntry> = audit
            .iter()
            .filter(|e| {
                if filter
                    .connector_name
                    .as_ref()
                    .is_some_and(|c| e.connector_name != *c)
                {
                    return false;
                }
                if filter.after.as_ref().is_some_and(|a| e.timestamp < *a) {
                    return false;
                }
                if filter.before.as_ref().is_some_and(|b| e.timestamp > *b) {
                    return false;
                }
                true
            })
            .take(filter.limit.unwrap_or(100) as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }

    pub(super) async fn export_audit_impl(
        &self,
        after: &DateTime<Utc>,
        before: &DateTime<Utc>,
    ) -> Result<Vec<AuditEntry>, StoreError> {
        let audit = self.audit.read().await;
        Ok(audit
            .iter()
            .filter(|e| e.timestamp >= *after && e.timestamp <= *before)
            .cloned()
            .collect())
    }

    pub(super) async fn delete_audit_before_impl(
        &self,
        before: &DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut audit = self.audit.write().await;
        let before_len = audit.len();
        audit.retain(|e| e.timestamp >= *before);
        Ok((before_len - audit.len()) as u64)
    }

    /// Walk the in-memory chain in `chain_seq` order. Mirrors the
    /// SQLite verifier path so chain checks behave identically across
    /// backends (used by the daemon-startup verifier when configured
    /// against the in-memory store, e.g. in tests).
    pub(super) async fn list_audit_chain_impl(&self) -> Result<Vec<AuditEntry>, StoreError> {
        let audit = self.audit.read().await;
        let mut out: Vec<AuditEntry> = audit.iter().cloned().collect();
        out.sort_by_key(|e| e.chain_seq);
        Ok(out)
    }
}
