use crate::action::SubTask;
use crate::cadence::AgentId;

use super::inbox::DirectInbox;

/// Convenience wrapper for posting an assigned task. Kept as a separate file
/// so future logic (e.g. emitting a protocol message on FormationBus when we
/// wire L3 fully) lives here, not in the inbox store.
pub fn assign(inbox: &DirectInbox, target: AgentId, task: SubTask) {
    inbox.push(target, task);
}
