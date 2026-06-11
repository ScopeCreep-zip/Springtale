//! CBBA round orchestrator — drives bundle + consensus to a conflict-free
//! assignment.
//!
//! Called once per replan trigger. Single-process, fully-connected neighbor
//! graph: every agent exchanges with every other on each sweep.

use std::collections::HashMap;

use crate::action::SubTask;
use crate::authority::{self, Unauthorized};
use crate::cadence::AgentId;
use crate::layer::LayerId;
use crate::momentum::MomentumTier;
use crate::routing::types::TaskId;

use super::bundle;
use super::consensus::{self, ConsensusState};
use super::convergence;
use super::types::{Bundle, ConvergenceStatus};

/// Per-agent input to the round: capabilities the agent carries.
pub struct AgentSpec {
    pub agent: AgentId,
    pub capabilities: Vec<crate::capability::CapabilityDecl>,
}

/// Final outcome of the round.
#[derive(Debug)]
pub enum ReplanOutcome {
    Converged {
        assignment: HashMap<TaskId, AgentId>,
        sweeps: u32,
        unassigned: Vec<TaskId>,
    },
    Stalled {
        assignment: HashMap<TaskId, AgentId>,
        sweeps: u32,
    },
    Unauthorized(Unauthorized),
}

/// Maximum consensus sweeps. Stall when exceeded — caller escalates to L6.
const MAX_SWEEPS: u32 = 32;

/// Run a full CBBA round.
///
/// - Phase 1: every agent greedily builds a bundle from `tasks` using its
///   own capabilities.
/// - Phase 2: sweeps of pairwise consensus. A sweep exchanges state between
///   every pair. Converges when no pair reports `Running` during a sweep.
pub fn run(agents: &[AgentSpec], tasks: &[SubTask], tier: MomentumTier) -> ReplanOutcome {
    if let Err(e) = authority::require(tier, LayerId::L5Replan) {
        return ReplanOutcome::Unauthorized(e);
    }

    // Phase 1 — local bundle per agent.
    let mut bundles: HashMap<AgentId, Bundle> = agents
        .iter()
        .map(|spec| {
            let bundle = bundle::build(spec.agent, tasks, &spec.capabilities);
            (spec.agent, bundle)
        })
        .collect();

    let mut states: HashMap<AgentId, ConsensusState> = bundles
        .iter()
        .map(|(id, bundle)| {
            let mut s = ConsensusState::new();
            s.seed_from_bundle(bundle);
            (*id, s)
        })
        .collect();

    // Phase 2 — iterate sweeps.
    let mut sweeps = 0;
    loop {
        sweeps += 1;
        if matches!(
            convergence::check_stall(sweeps, MAX_SWEEPS),
            ConvergenceStatus::Stalled
        ) {
            return ReplanOutcome::Stalled {
                assignment: finalize(&states),
                sweeps,
            };
        }

        let sweep_status = one_sweep(agents, &mut bundles, &mut states);
        match convergence::fold_sweep(&sweep_status) {
            ConvergenceStatus::Converged => {
                let assignment = finalize(&states);
                let unassigned = tasks
                    .iter()
                    .map(|t| t.id)
                    .filter(|id| !assignment.contains_key(id))
                    .collect();
                return ReplanOutcome::Converged {
                    assignment,
                    sweeps,
                    unassigned,
                };
            }
            ConvergenceStatus::Stalled => {
                return ReplanOutcome::Stalled {
                    assignment: finalize(&states),
                    sweeps,
                };
            }
            ConvergenceStatus::Running => continue,
        }
    }
}

fn one_sweep(
    agents: &[AgentSpec],
    bundles: &mut HashMap<AgentId, Bundle>,
    states: &mut HashMap<AgentId, ConsensusState>,
) -> Vec<ConvergenceStatus> {
    let ids: Vec<AgentId> = agents.iter().map(|a| a.agent).collect();
    let mut statuses = Vec::new();
    for (i, local) in ids.iter().enumerate() {
        for neighbor in ids.iter().skip(i + 1) {
            // Snapshot neighbor state to avoid a second mutable borrow.
            let neighbor_state = states.get(neighbor).cloned().unwrap_or_default();
            if let (Some(local_bundle), Some(local_state)) =
                (bundles.get_mut(local), states.get_mut(local))
            {
                statuses.push(consensus::round(local_bundle, local_state, &neighbor_state));
            }

            // And vice versa — information flows both ways.
            let local_snapshot = states.get(local).cloned().unwrap_or_default();
            if let (Some(neighbor_bundle), Some(neighbor_state)) =
                (bundles.get_mut(neighbor), states.get_mut(neighbor))
            {
                statuses.push(consensus::round(
                    neighbor_bundle,
                    neighbor_state,
                    &local_snapshot,
                ));
            }
        }
    }
    statuses
}

fn finalize(states: &HashMap<AgentId, ConsensusState>) -> HashMap<TaskId, AgentId> {
    // Every agent's ConsensusState should agree after convergence — pick any
    // as the authoritative view. Conflicts at this stage are a bug, not a
    // recoverable condition, so we don't attempt merging.
    if let Some((_, state)) = states.iter().next() {
        state.winning_agents.clone()
    } else {
        HashMap::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_task(connector: &str, priority: u8) -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: crate::capability::CapabilityDecl::new(connector),
            action_name: "act".to_owned(),
            params: serde_json::json!({}),
            priority,
            assigned_to: None,
            description: String::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn unauthorized_below_fever() {
        let out = run(&[], &[], MomentumTier::Hot);
        assert!(matches!(out, ReplanOutcome::Unauthorized(_)));
    }

    #[test]
    fn converges_with_disjoint_capabilities() {
        let a = AgentId::new();
        let b = AgentId::new();
        let tasks = vec![make_task("github", 1), make_task("slack", 1)];
        let agents = vec![
            AgentSpec {
                agent: a,
                capabilities: vec!["github".into()],
            },
            AgentSpec {
                agent: b,
                capabilities: vec!["slack".into()],
            },
        ];
        let out = run(&agents, &tasks, MomentumTier::Fever);
        match out {
            ReplanOutcome::Converged {
                assignment,
                unassigned,
                ..
            } => {
                assert_eq!(assignment.len(), 2);
                assert!(unassigned.is_empty());
            }
            other => panic!("expected Converged, got {other:?}"),
        }
    }
}
