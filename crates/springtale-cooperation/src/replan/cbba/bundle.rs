//! CBBA Phase 1 — greedy local bundle building.
//!
//! Each agent builds an ordered bundle of tasks it would prefer to own,
//! computing marginal utility with geometric decay. Geometric decay
//! `base * decay_rate^position` guarantees Diminishing Marginal Gain (DMG):
//! every later slot contributes less than every earlier slot, so adding a
//! task never increases the bid on any task already in the bundle.
//!
//! Reference: Choi, Brunet & How, "Consensus-based decentralized auctions
//! for robust task allocation" — MIT ACL.

use crate::action::SubTask;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::routing::types::TaskId;

use super::types::Bundle;

/// Decay applied per bundle position. At position 0 the full base utility
/// counts; at position 1 it's scaled by `DECAY`; position 2 by `DECAY²`; …
const DECAY: f32 = 0.75;

/// Maximum bundle size per agent. Bounded so one agent can't monopolize
/// allocation. Caller can override via `build_with_capacity`.
const DEFAULT_CAPACITY: usize = 4;

/// Build the best greedy bundle `agent` can claim from `tasks`.
///
/// Returns an empty bundle when no task is capability-feasible — a
/// non-capable agent contributes nothing to the consensus round.
pub fn build(agent: AgentId, tasks: &[SubTask], capabilities: &[CapabilityDecl]) -> Bundle {
    build_with_capacity(agent, tasks, capabilities, DEFAULT_CAPACITY)
}

pub fn build_with_capacity(
    agent: AgentId,
    tasks: &[SubTask],
    capabilities: &[CapabilityDecl],
    capacity: usize,
) -> Bundle {
    let feasible: Vec<(TaskId, f32)> = tasks
        .iter()
        .filter(|t| capabilities.iter().any(|c| c == &t.target_connector))
        .map(|t| (t.id, base_utility(t)))
        .collect();

    let mut bundle = Bundle {
        owner: agent,
        tasks: Vec::new(),
        bids: Vec::new(),
        iteration: 0,
    };

    let mut remaining: Vec<(TaskId, f32)> = feasible;
    while bundle.tasks.len() < capacity {
        let position = bundle.tasks.len();
        let decay_factor = DECAY.powi(position as i32);

        let Some((idx, (task_id, base))) = remaining
            .iter()
            .enumerate()
            .map(|(i, (id, base))| (i, (*id, *base)))
            .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
        else {
            break;
        };

        let marginal = base * decay_factor;
        if marginal <= f32::EPSILON {
            break;
        }

        bundle.tasks.push(task_id);
        bundle.bids.push(marginal);
        remaining.swap_remove(idx);
    }

    bundle
}

/// Base utility of a task before decay. Lower priority number = higher base.
/// Callers wanting more signal (connector affinity, SLA) extend by wrapping
/// this in their own Consideration.
pub fn base_utility(task: &SubTask) -> f32 {
    // Priority 1 → 1.0; priority 10 → 0.1; clamped.
    let p = task.priority.clamp(1, 10) as f32;
    1.0 - (p - 1.0) / 10.0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_task(connector: &str, priority: u8) -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: CapabilityDecl::new(connector),
            action_name: "act".to_owned(),
            params: serde_json::json!({}),
            priority,
            assigned_to: None,
            description: String::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn non_capable_agent_builds_empty_bundle() {
        let agent = AgentId::new();
        let tasks = vec![make_task("github", 1)];
        let bundle = build(agent, &tasks, &[CapabilityDecl::new("slack")]);
        assert!(bundle.tasks.is_empty());
    }

    #[test]
    fn bundle_orders_by_priority() {
        let agent = AgentId::new();
        let low = make_task("github", 5);
        let high = make_task("github", 1);
        let tasks = vec![low.clone(), high.clone()];
        let bundle = build(agent, &tasks, &[CapabilityDecl::new("github")]);
        assert_eq!(bundle.tasks, vec![high.id, low.id]);
    }

    #[test]
    fn bids_decay_geometrically() {
        let agent = AgentId::new();
        let tasks = vec![make_task("github", 1), make_task("github", 1)];
        let bundle = build(agent, &tasks, &[CapabilityDecl::new("github")]);
        assert_eq!(bundle.bids.len(), 2);
        // Position 1 bid = DECAY × Position 0 bid.
        assert!(bundle.bids[0] > bundle.bids[1]);
    }

    #[test]
    fn capacity_bounds_bundle_size() {
        let agent = AgentId::new();
        let tasks: Vec<SubTask> = (0..10).map(|_| make_task("github", 1)).collect();
        let bundle = build_with_capacity(agent, &tasks, &[CapabilityDecl::new("github")], 3);
        assert_eq!(bundle.tasks.len(), 3);
    }

    #[test]
    fn dmg_invariant_earlier_bids_untouched() {
        // DMG: adding task at position k does not change bids at positions < k.
        // Because we compute bid = base * decay^position at insert time, and
        // never revisit earlier slots, the invariant holds by construction.
        let agent = AgentId::new();
        let tasks = vec![
            make_task("github", 1),
            make_task("github", 2),
            make_task("github", 3),
        ];
        let full = build(agent, &tasks, &[CapabilityDecl::new("github")]);
        let first_two =
            build_with_capacity(agent, &tasks[0..2], &[CapabilityDecl::new("github")], 2);
        // First two slots must have identical bids regardless of whether a
        // third task exists — that is the DMG property for the bundle phase.
        assert_eq!(&full.bids[..2], &first_two.bids[..]);
    }
}
