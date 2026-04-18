//! L1 scan-and-claim step — scan capability-indexed queues, attempt atomic
//! claim. Delegates to `routing::scan::scan_and_claim`.

use crate::agent_loop::AgentTickResult;
use crate::authority;
use crate::cadence::{ActionDescriptor, AgentId};
use crate::capability::CapabilityDecl;
use crate::layer::LayerId;
use crate::momentum::MomentumTier;
use crate::routing::claim::ClaimManager;
use crate::routing::index::CapabilityIndex;
use crate::routing::scan;
use crate::routing::types::TaskClaim;

/// Scan result — both the tick result and the claim proof so the caller can
/// track ownership lifetime.
pub struct ScanOutcome {
    pub result: AgentTickResult,
    pub claim: TaskClaim,
}

/// Scan the capability-indexed task pool and attempt to claim the best
/// match. Returns `Some(ScanOutcome)` if a task was claimed; `None`
/// on empty pool or lost race. The returned `TaskClaim` carries the ownership
/// proof — callers use it to release or complete the task.
pub fn step_scan(
    index: &CapabilityIndex,
    claims: &ClaimManager,
    agent_id: AgentId,
    capabilities: &[CapabilityDecl],
    tier: MomentumTier,
) -> Option<ScanOutcome> {
    if !authority::allows(tier, LayerId::L1Routine) {
        return None;
    }

    let (task, claim) = scan::scan_and_claim(index, claims, agent_id, capabilities).ok()??;

    Some(ScanOutcome {
        result: AgentTickResult {
            agent_id,
            action: Some(ActionDescriptor {
                kind: "task_claimed".to_owned(),
                target: Some(task.capability().to_owned()),
                payload_hash: 0,
            }),
            alignment: 1.0,
            interference_with: vec![],
            task_claimed: Some(task.task),
            task_completed: false,
        },
        claim,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::action::SubTask;
    use crate::routing::types::PriorityTask;

    fn task(connector: &str, priority: u8) -> PriorityTask {
        PriorityTask::new(SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: crate::capability::CapabilityDecl::new(connector),
            action_name: "act".to_owned(),
            params: serde_json::json!({}),
            priority,
            assigned_to: None,
            description: String::new(),
        })
    }

    #[test]
    fn claims_best_matching_task() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        let agent = AgentId::new();
        index.insert(task("github", 1));
        index.insert(task("github", 5));
        let outcome = step_scan(&index, &claims, agent, &["github".into()], MomentumTier::Hot);
        assert!(outcome.is_some());
        let o = outcome.unwrap();
        assert_eq!(o.result.task_claimed.as_ref().unwrap().priority, 1);
        assert_eq!(o.claim.owner, agent);
    }

    #[test]
    fn no_matching_tasks_returns_none() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        let result = step_scan(
            &index,
            &claims,
            AgentId::new(),
            &["github".into()],
            MomentumTier::Hot,
        );
        assert!(result.is_none());
    }

    #[test]
    fn lost_race_returns_none() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        let t = task("github", 1);
        let tid = t.id();
        index.insert(t);
        let winner = AgentId::new();
        claims.try_claim(tid, winner).unwrap();
        let loser = AgentId::new();
        let result = step_scan(&index, &claims, loser, &["github".into()], MomentumTier::Hot);
        assert!(result.is_none());
    }

    #[test]
    fn always_allowed_at_any_tier() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        index.insert(task("github", 1));
        let result = step_scan(
            &index,
            &claims,
            AgentId::new(),
            &["github".into()],
            MomentumTier::Cold,
        );
        assert!(result.is_some(), "L1 is allowed at Cold");
    }
}
