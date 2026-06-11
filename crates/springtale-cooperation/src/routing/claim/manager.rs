use std::time::Instant;

use dashmap::DashMap;

use crate::cadence::AgentId;
use crate::routing::types::{RoutingError, TaskClaim, TaskId};

/// CAS-guarded claim map. First agent to insert wins; subsequent inserts
/// return `RoutingError::LostRace`.
#[derive(Debug, Default)]
pub struct ClaimManager {
    claims: DashMap<TaskId, TaskClaim>,
}

impl ClaimManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomic claim. Uses DashMap entry API — equivalent to `compare-and-set`
    /// where the "expected" value is absence.
    pub fn try_claim(&self, task_id: TaskId, agent: AgentId) -> Result<TaskClaim, RoutingError> {
        super::cas::try_claim(&self.claims, task_id, agent)
    }

    pub fn release(&self, task_id: TaskId) {
        self.claims.remove(&task_id);
    }

    pub fn owner_of(&self, task_id: TaskId) -> Option<AgentId> {
        self.claims.get(&task_id).map(|c| c.owner)
    }

    pub fn stale_claims(&self, max_age: std::time::Duration, now: Instant) -> Vec<TaskId> {
        self.claims
            .iter()
            .filter(|c| now.duration_since(c.value().claimed_at) > max_age)
            .map(|c| *c.key())
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn first_claim_wins() {
        let mgr = ClaimManager::new();
        let tid = uuid::Uuid::new_v4();
        let a = AgentId::new();
        assert!(mgr.try_claim(tid, a).is_ok());
        assert_eq!(mgr.owner_of(tid), Some(a));
    }

    #[test]
    fn second_claim_loses_race() {
        let mgr = ClaimManager::new();
        let tid = uuid::Uuid::new_v4();
        let a = AgentId::new();
        let b = AgentId::new();
        mgr.try_claim(tid, a).unwrap();
        assert!(matches!(
            mgr.try_claim(tid, b),
            Err(RoutingError::LostRace(_))
        ));
    }

    #[test]
    fn release_allows_reclaim() {
        let mgr = ClaimManager::new();
        let tid = uuid::Uuid::new_v4();
        let a = AgentId::new();
        let b = AgentId::new();
        mgr.try_claim(tid, a).unwrap();
        mgr.release(tid);
        assert!(mgr.try_claim(tid, b).is_ok());
        assert_eq!(mgr.owner_of(tid), Some(b));
    }

    #[test]
    fn owner_of_missing_returns_none() {
        let mgr = ClaimManager::new();
        assert!(mgr.owner_of(uuid::Uuid::new_v4()).is_none());
    }
}
