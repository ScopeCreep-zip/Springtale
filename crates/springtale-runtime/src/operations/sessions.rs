//! Bot conversation-session listing.

use serde::Serialize;
use specta::Type;
use springtale_store::StorageBackend;

use crate::error::OperationError;

/// Per-user, per-channel conversation state.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SessionInfo {
    pub user_id: String,
    pub channel_id: String,
    pub last_bot_message: Option<String>,
    pub pending_command: Option<String>,
    /// Arbitrary handler state as a JSON string.
    pub state_data: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-update timestamp.
    pub updated_at: String,
}

/// List all active bot sessions.
pub async fn list(store: &dyn StorageBackend) -> Result<Vec<SessionInfo>, OperationError> {
    let sessions = store.list_sessions().await.map_err(OperationError::Store)?;
    Ok(sessions
        .into_iter()
        .map(|s| SessionInfo {
            user_id: s.user_id,
            channel_id: s.channel_id,
            last_bot_message: s.last_bot_message,
            pending_command: s.pending_command,
            state_data: s.state_data,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        })
        .collect())
}
