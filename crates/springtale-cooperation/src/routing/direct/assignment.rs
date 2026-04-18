use crate::cadence::AgentId;
use crate::routing::types::TaskId;

use super::inbox::DirectInbox;

/// Convenience wrapper for posting an assigned task. Kept as a separate file
/// so future logic (e.g. emitting a protocol message on FormationBus when we
/// wire L3 fully) lives here, not in the inbox store.
pub fn assign(inbox: &DirectInbox, target: AgentId, task_id: TaskId) {
    inbox.push(target, task_id);
}
