//! Bot-level status and memory summaries.

use serde::Serialize;
use specta::Type;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Runtime status summary.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BotStatus {
    /// Always true when the runtime answered.
    pub running: bool,
    /// Loaded connectors.
    pub connectors: usize,
    /// Rules known to the engine.
    pub rules: usize,
    /// Formations in the store.
    pub formations: usize,
}

/// Per-session memory metadata — never decrypted content.
///
/// Per the zero-knowledge architecture pattern (Bitwarden, Signal,
/// Matrix), the API exposes who/when/how-much, never what was said.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SessionMemorySummary {
    pub user_id: String,
    pub channel_id: String,
    /// RFC 3339 timestamp of the last activity.
    pub last_active: String,
    /// Whether any memory entry exists — not how many, not what.
    pub has_entries: bool,
}

/// Summarise the running colony: connectors, rules, formations.
pub async fn status(state: &RuntimeState) -> Result<BotStatus, OperationError> {
    let connectors = {
        let registry = state.registry.read().await;
        registry.list().len()
    };
    let rules = {
        let engine = state.engine.read().await;
        engine.list_rules().len()
    };
    let formations = state
        .store
        .list_formations()
        .await
        .map_err(OperationError::Store)?
        .len();

    Ok(BotStatus {
        running: true,
        connectors,
        rules,
        formations,
    })
}

/// Session metadata only — no decrypted conversation content.
pub async fn memory_summary(
    state: &RuntimeState,
) -> Result<Vec<SessionMemorySummary>, OperationError> {
    let sessions = state
        .store
        .list_sessions()
        .await
        .map_err(OperationError::Store)?;

    let mut summaries = Vec::with_capacity(sessions.len());
    for session in &sessions {
        let entries = state
            .store
            .get_memory(&session.user_id, &session.channel_id, 1)
            .await
            .unwrap_or_default();
        summaries.push(SessionMemorySummary {
            user_id: session.user_id.clone(),
            channel_id: session.channel_id.clone(),
            last_active: session.updated_at.to_rfc3339(),
            has_entries: !entries.is_empty(),
        });
    }
    Ok(summaries)
}
