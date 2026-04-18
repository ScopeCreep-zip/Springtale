use std::time::Instant;

use dashmap::DashMap;

use crate::cadence::AgentId;
use crate::routing::types::{RoutingError, TaskClaim, TaskId};

/// Internal CAS primitive. Isolated in its own file so the race semantic
/// is in exactly one place and easy to audit.
pub(super) fn try_claim(
    claims: &DashMap<TaskId, TaskClaim>,
    task_id: TaskId,
    agent: AgentId,
) -> Result<TaskClaim, RoutingError> {
    use dashmap::mapref::entry::Entry;
    match claims.entry(task_id) {
        Entry::Vacant(v) => {
            let claim = TaskClaim {
                task_id,
                owner: agent,
                claimed_at: Instant::now(),
            };
            v.insert(claim.clone());
            Ok(claim)
        }
        Entry::Occupied(_) => Err(RoutingError::LostRace(task_id)),
    }
}
