//! CBBA Phase 2 — neighbor-gossip consensus.
//!
//! Each agent maintains local "winning bid" (`y`) and "winning agent" (`z`)
//! tables per task. Neighbors exchange these tables; when a neighbor holds a
//! higher bid, the local agent releases the task from its bundle. A single
//! round runs until no changes occur.
//!
//! Simplification vs. the canonical paper: no timestamps/rebroadcast tracking
//! — resolution is strictly "higher bid wins, ties broken by agent-id." For
//! the single-process formation this is sufficient and deterministic.

use std::collections::HashMap;

use crate::cadence::AgentId;
use crate::routing::types::TaskId;

use super::types::{Bundle, ConvergenceStatus};

/// Winning-bid / winning-agent tables.
#[derive(Debug, Default, Clone)]
pub struct ConsensusState {
    pub winning_bids: HashMap<TaskId, f32>,
    pub winning_agents: HashMap<TaskId, AgentId>,
}

impl ConsensusState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the tables from the local agent's own bundle — the agent starts
    /// as the (provisional) winner for each task in its bundle.
    pub fn seed_from_bundle(&mut self, bundle: &Bundle) {
        for (id, bid) in bundle.tasks.iter().zip(bundle.bids.iter()) {
            self.winning_bids.insert(*id, *bid);
            self.winning_agents.insert(*id, bundle.owner);
        }
    }
}

/// Run one consensus round between `local` and `neighbor`. Returns whether
/// anything changed locally — callers iterate until `Converged`.
pub fn round(
    local_bundle: &mut Bundle,
    local_state: &mut ConsensusState,
    neighbor_state: &ConsensusState,
) -> ConvergenceStatus {
    let mut changed = false;

    // Consider every task either side knows about.
    let mut seen: Vec<TaskId> = local_state.winning_bids.keys().copied().collect();
    for id in neighbor_state.winning_bids.keys() {
        if !seen.contains(id) {
            seen.push(*id);
        }
    }

    for task_id in seen {
        let local_bid = local_state.winning_bids.get(&task_id).copied();
        let neighbor_bid = neighbor_state.winning_bids.get(&task_id).copied();
        let local_owner = local_state.winning_agents.get(&task_id).copied();
        let neighbor_owner = neighbor_state.winning_agents.get(&task_id).copied();

        match (local_bid, neighbor_bid, neighbor_owner) {
            (None, Some(nb), Some(no)) => {
                // Task new to us — adopt the neighbor's record.
                local_state.winning_bids.insert(task_id, nb);
                local_state.winning_agents.insert(task_id, no);
                changed = true;
            }
            (Some(lb), Some(nb), Some(no))
                if higher_bid(nb, no, lb, local_owner.unwrap_or(no)) =>
            {
                // Neighbor's bid wins; update local tables.
                local_state.winning_bids.insert(task_id, nb);
                local_state.winning_agents.insert(task_id, no);
                if local_bundle.owner != no {
                    release_from_bundle(local_bundle, task_id);
                }
                changed = true;
            }
            _ => {}
        }
    }

    local_bundle.iteration = local_bundle.iteration.saturating_add(1);
    if changed {
        ConvergenceStatus::Running
    } else {
        ConvergenceStatus::Converged
    }
}

/// Higher-bid-wins with stable tie-break by agent-id. Returning `true` means
/// the neighbor's (bid, owner) pair should replace the local (bid, owner).
fn higher_bid(nb: f32, no: AgentId, lb: f32, lo: AgentId) -> bool {
    match nb.partial_cmp(&lb) {
        Some(std::cmp::Ordering::Greater) => true,
        Some(std::cmp::Ordering::Equal) => no.0 < lo.0,
        _ => false,
    }
}

fn release_from_bundle(bundle: &mut Bundle, task_id: TaskId) {
    if let Some(idx) = bundle.tasks.iter().position(|t| *t == task_id) {
        bundle.tasks.remove(idx);
        if idx < bundle.bids.len() {
            bundle.bids.remove(idx);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn state(entries: &[(TaskId, f32, AgentId)]) -> ConsensusState {
        let mut s = ConsensusState::new();
        for (id, bid, owner) in entries {
            s.winning_bids.insert(*id, *bid);
            s.winning_agents.insert(*id, *owner);
        }
        s
    }

    #[test]
    fn neighbor_with_higher_bid_wins() {
        let t = uuid::Uuid::new_v4();
        let a = AgentId::new();
        let b = AgentId::new();
        let mut local_bundle = Bundle {
            owner: a,
            tasks: vec![t],
            bids: vec![0.5],
            iteration: 0,
        };
        let mut local_state = state(&[(t, 0.5, a)]);
        let neighbor_state = state(&[(t, 0.8, b)]);

        let status = round(&mut local_bundle, &mut local_state, &neighbor_state);
        assert_eq!(status, ConvergenceStatus::Running);
        assert_eq!(local_bundle.tasks.len(), 0, "lost task released");
        assert_eq!(local_state.winning_agents.get(&t), Some(&b));
    }

    #[test]
    fn local_with_higher_bid_keeps_task() {
        let t = uuid::Uuid::new_v4();
        let a = AgentId::new();
        let b = AgentId::new();
        let mut local_bundle = Bundle {
            owner: a,
            tasks: vec![t],
            bids: vec![0.9],
            iteration: 0,
        };
        let mut local_state = state(&[(t, 0.9, a)]);
        let neighbor_state = state(&[(t, 0.4, b)]);

        let status = round(&mut local_bundle, &mut local_state, &neighbor_state);
        assert_eq!(status, ConvergenceStatus::Converged);
        assert_eq!(local_bundle.tasks, vec![t]);
    }

    #[test]
    fn new_task_from_neighbor_is_adopted_without_claim() {
        let t = uuid::Uuid::new_v4();
        let a = AgentId::new();
        let b = AgentId::new();
        let mut local_bundle = Bundle {
            owner: a,
            tasks: vec![],
            bids: vec![],
            iteration: 0,
        };
        let mut local_state = ConsensusState::new();
        let neighbor_state = state(&[(t, 0.7, b)]);

        let status = round(&mut local_bundle, &mut local_state, &neighbor_state);
        assert_eq!(status, ConvergenceStatus::Running);
        // We learned about the task but aren't the owner → bundle unchanged.
        assert!(local_bundle.tasks.is_empty());
        assert_eq!(local_state.winning_agents.get(&t), Some(&b));
    }
}
