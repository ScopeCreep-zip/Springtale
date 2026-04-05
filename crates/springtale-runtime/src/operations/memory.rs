//! Memory operations — audit and compact bot memory.

use serde::Serialize;
use springtale_store::StorageBackend;
use springtale_store::schema::bot::SessionRow;

use crate::error::OperationError;

/// Memory audit summary.
#[derive(Debug, Serialize)]
pub struct MemoryAuditResult {
    /// Active bot sessions.
    pub sessions: Vec<SessionRow>,
    /// Human-readable summary of memory state.
    pub total_memory_note: String,
}

/// Audit bot memory — list all sessions and memory entry counts.
pub async fn audit_memory(store: &dyn StorageBackend) -> Result<MemoryAuditResult, OperationError> {
    let sessions = store.list_sessions().await.map_err(OperationError::Store)?;
    Ok(MemoryAuditResult {
        total_memory_note: format!("{} active sessions", sessions.len()),
        sessions,
    })
}

/// Compact memory for all sessions — delete oldest entries beyond the given limit.
///
/// Returns the total number of entries deleted across all sessions.
pub async fn compact_memory(
    store: &dyn StorageBackend,
    max_entries_per_session: usize,
) -> Result<u64, OperationError> {
    let sessions = store.list_sessions().await.map_err(OperationError::Store)?;
    let mut total_deleted = 0u64;
    for session in &sessions {
        let deleted = store
            .compact_memory(
                &session.user_id,
                &session.channel_id,
                max_entries_per_session,
            )
            .await
            .map_err(OperationError::Store)?;
        total_deleted += deleted;
    }
    Ok(total_deleted)
}
