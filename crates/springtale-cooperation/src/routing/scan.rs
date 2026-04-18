//! L1 scan policy — composed from capability index + claim manager.
//!
//! This is the "scan then claim" pattern from RimWorld's JobGiver_Work:
//! peek the best candidate across capability buckets, attempt atomic claim,
//! and fall through on race loss so the agent can try again next tick.

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::routing::claim::ClaimManager;
use crate::routing::index::CapabilityIndex;
use crate::routing::types::{PriorityTask, RoutingError, TaskClaim};

/// Scan the capability index for the best-priority match, then attempt an
/// atomic claim. Returns `Ok(None)` when nothing was found or the race was
/// lost — callers treat those the same (fall through to idle / next tick).
pub fn scan_and_claim(
    index: &CapabilityIndex,
    claims: &ClaimManager,
    agent: AgentId,
    capabilities: &[CapabilityDecl],
) -> Result<Option<(PriorityTask, TaskClaim)>, RoutingError> {
    let Some(candidate) = index.peek_best(capabilities) else {
        return Ok(None);
    };
    match claims.try_claim(candidate.id(), agent) {
        Ok(claim) => Ok(Some((candidate, claim))),
        Err(RoutingError::LostRace(_)) => Ok(None),
        Err(e) => Err(e),
    }
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
    fn scan_finds_and_claims() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        let agent = AgentId::new();
        index.insert(task("github", 1));
        let result = scan_and_claim(&index, &claims, agent, &["github".into()]).unwrap();
        assert!(result.is_some());
        let (pt, claim) = result.unwrap();
        assert_eq!(pt.priority(), 1);
        assert_eq!(claim.owner, agent);
    }

    #[test]
    fn scan_returns_none_on_empty_index() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        let result =
            scan_and_claim(&index, &claims, AgentId::new(), &["github".into()]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scan_returns_none_when_race_lost() {
        let index = CapabilityIndex::new();
        let claims = ClaimManager::new();
        let t = task("github", 1);
        let tid = t.id();
        index.insert(t);
        let winner = AgentId::new();
        claims.try_claim(tid, winner).unwrap();
        let loser = AgentId::new();
        let result = scan_and_claim(&index, &claims, loser, &["github".into()]).unwrap();
        assert!(result.is_none());
    }
}
