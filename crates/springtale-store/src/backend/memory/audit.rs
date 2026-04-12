use chrono::{DateTime, Utc};

use crate::error::StoreError;
use crate::schema::audit::{AuditEntry, AuditFilter};

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn insert_audit_entry_impl(
        &self,
        entry: &AuditEntry,
    ) -> Result<(), StoreError> {
        let mut audit = self.audit.write().await;
        audit.push(entry.clone());
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
}
