use serde_json::Value;

use crate::cadence::AgentId;
use crate::types::WorkspaceKey;

/// Concurrent key/value workspace shared across a formation.
///
/// Implementations are free to store audit trails, revision numbers, etc. —
/// the trait only requires the read/write surface consumers use directly.
pub trait Workspace: Send + Sync {
    fn read(&self, key: &str) -> Option<Value>;
    fn write(&self, key: WorkspaceKey, value: Value, writer: AgentId);
}
