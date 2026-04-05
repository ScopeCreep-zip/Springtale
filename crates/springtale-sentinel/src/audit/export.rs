use std::sync::Arc;

use chrono::{DateTime, Utc};
use springtale_store::{AuditEntry, AuditFilter, StorageBackend};

use crate::error::SentinelError;

/// Export audit trail entries within a time range.
pub async fn export_audit(
    store: &Arc<dyn StorageBackend>,
    after: &DateTime<Utc>,
    before: &DateTime<Utc>,
) -> Result<Vec<AuditEntry>, SentinelError> {
    let entries = store.export_audit(after, before).await?;
    Ok(entries)
}

/// List audit entries with optional filters.
pub async fn list_audit(
    store: &Arc<dyn StorageBackend>,
    filter: &AuditFilter,
) -> Result<Vec<AuditEntry>, SentinelError> {
    let entries = store.list_audit_entries(filter).await?;
    Ok(entries)
}
