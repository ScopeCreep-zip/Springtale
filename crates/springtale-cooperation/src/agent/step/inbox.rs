//! L3 direct-handoff step — poll the agent's DirectInbox for tasks assigned
//! specifically to them. Takes priority over the general scan (L1).

use crate::agent_loop::AgentTickResult;
use crate::authority;
use crate::cadence::{ActionDescriptor, AgentId};
use crate::layer::LayerId;
use crate::momentum::MomentumTier;
use crate::routing::direct::DirectInbox;
use crate::routing::types::TaskId;

/// Check if a task has been directly assigned to this agent.
///
/// Returns `Some(AgentTickResult)` with the task id if one is waiting;
/// `None` to fall through to the L1 scan. The caller (event loop) resolves
/// the task id back to a SubTask via the CapabilityIndex or blackboard.
pub fn step_check_inbox(
    inbox: &DirectInbox,
    agent_id: AgentId,
    tier: MomentumTier,
) -> Option<(AgentTickResult, TaskId)> {
    if !authority::allows(tier, LayerId::L3Direct) {
        return None;
    }

    let task_id = inbox.poll(agent_id)?;

    Some((
        AgentTickResult {
            agent_id,
            action: Some(ActionDescriptor {
                kind: "direct_handoff".to_owned(),
                target: Some(task_id.to_string()),
                payload_hash: 0,
            }),
            alignment: 1.0,
            interference_with: vec![],
            task_claimed: None,
            task_completed: false,
        },
        task_id,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn empty_inbox_returns_none() {
        let inbox = DirectInbox::new();
        assert!(step_check_inbox(&inbox, AgentId::new(), MomentumTier::Hot).is_none());
    }

    #[test]
    fn assigned_task_returned() {
        let inbox = DirectInbox::new();
        let agent = AgentId::new();
        let tid = uuid::Uuid::new_v4();
        inbox.push(agent, tid);
        let (result, returned_id) =
            step_check_inbox(&inbox, agent, MomentumTier::Hot).unwrap();
        assert_eq!(returned_id, tid);
        assert_eq!(result.agent_id, agent);
    }

    #[test]
    fn cold_tier_blocks_direct_handoff() {
        let inbox = DirectInbox::new();
        let agent = AgentId::new();
        inbox.push(agent, uuid::Uuid::new_v4());
        assert!(step_check_inbox(&inbox, agent, MomentumTier::Cold).is_none());
    }
}
