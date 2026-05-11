//! Step 10b — L5 CBBA replan executor (B3).
//!
//! Per `pure-noodling-biscuit.md` work-order item 6 + `COOPERATION.md §15`:
//! when the supervisor returns `SupervisionAction::TriggerReplan` (set on
//! `formation.needs_replan` by B10), this step runs a full CBBA round to
//! produce a conflict-free reassignment of the formation's tasks.
//!
//! Outcomes:
//! - **Converged** — assignments applied to the blackboard
//!   (`SubTask::assigned_to` rewritten via re-post). Flag cleared.
//! - **Stalled** — partial assignment applied; flag stays set so B1's
//!   intervention layer sees `cbba_stalled = true` and decides whether to
//!   escalate to user.
//! - **Unauthorized** — formation tier below Fever; flag cleared (replan
//!   is Fever-only, the request is moot at this tier).
//!
//! Runs after `transformation::run` (step 10) so role transformations have
//! already settled, and before `check_interventions::run` so L6 sees the
//! up-to-date `needs_replan` signal.

use crate::cooperation::blackboard::trait_::Blackboard;
use tokio::sync::broadcast;

use crate::cooperation::formation::Formation;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::events::{
    self, CooperationEvent, CooperationEventEnvelope, ReplanOutcomeSummary,
};
use springtale_cooperation::replan::cbba::{self, AgentSpec, ReplanOutcome};
use springtale_cooperation::routing::types::TaskId;

pub fn run(
    formation: &mut Formation,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    if !formation.needs_replan {
        return;
    }
    events::emit(
        cooperation_tx,
        CooperationEvent::CbbaReplanRequested {
            formation_id: formation.id,
            reason: "supervisor flagged needs_replan".into(),
        },
    );

    let agents: Vec<AgentSpec> = formation
        .members
        .iter()
        .filter(|m| m.is_operational())
        .map(|m| AgentSpec {
            agent: m.agent_id,
            capabilities: m.capabilities.clone(),
        })
        .collect();
    if agents.is_empty() {
        // No operational members — nothing to assign. Clear the flag so
        // L6 doesn't loop on it.
        formation.needs_replan = false;
        return;
    }

    let tasks = formation.blackboard.scan_tasks(&[]);
    if tasks.is_empty() {
        formation.needs_replan = false;
        return;
    }

    let outcome = cbba::run(&agents, &tasks, formation.momentum.tier);
    let formation_id = formation.id.0.to_string();
    match outcome {
        ReplanOutcome::Converged {
            assignment,
            sweeps,
            unassigned,
        } => {
            apply_assignments(formation, &assignment);
            tracing::info!(
                formation = %formation_id,
                sweeps,
                assigned = assignment.len(),
                unassigned = unassigned.len(),
                "cbba replan converged"
            );
            let summary = ReplanOutcomeSummary {
                status: "converged",
                sweeps,
                assigned: assignment.len() as u32,
                unassigned: unassigned.len() as u32,
            };
            events::emit(
                cooperation_tx,
                CooperationEvent::CbbaReplanResolved {
                    formation_id: formation.id,
                    outcome: summary,
                },
            );
            formation.needs_replan = false;
        }
        ReplanOutcome::Stalled { assignment, sweeps } => {
            apply_assignments(formation, &assignment);
            tracing::warn!(
                formation = %formation_id,
                sweeps,
                partial = assignment.len(),
                "cbba replan stalled — leaving needs_replan set for L6 escalation"
            );
            let summary = ReplanOutcomeSummary {
                status: "stalled",
                sweeps,
                assigned: assignment.len() as u32,
                unassigned: 0,
            };
            events::emit(
                cooperation_tx,
                CooperationEvent::CbbaReplanResolved {
                    formation_id: formation.id,
                    outcome: summary,
                },
            );
            // Flag stays set so check_interventions sees `cbba_stalled = true`.
        }
        ReplanOutcome::Unauthorized(reason) => {
            tracing::debug!(
                formation = %formation_id,
                ?reason,
                "cbba replan unauthorized at current tier — clearing flag"
            );
            let summary = ReplanOutcomeSummary {
                status: "unauthorized",
                sweeps: 0,
                assigned: 0,
                unassigned: 0,
            };
            events::emit(
                cooperation_tx,
                CooperationEvent::CbbaReplanResolved {
                    formation_id: formation.id,
                    outcome: summary,
                },
            );
            formation.needs_replan = false;
        }
    }
}

/// Apply CBBA's task→agent assignment back onto the blackboard. CBBA
/// returns logical assignments; the blackboard owns the claim ledger.
/// Re-posting the task with `assigned_to: Some(agent)` lets the agent's
/// next `scan_and_claim` see the directed work.
fn apply_assignments(formation: &Formation, assignment: &std::collections::HashMap<TaskId, AgentId>) {
    use springtale_cooperation::action::SubTask;

    // Index existing blackboard tasks by their own id so we can rewrite
    // assigned_to in place. `TaskId` is a `uuid::Uuid` type alias so the
    // assignment keys match SubTask::id directly.
    let current = formation.blackboard.scan_tasks(&[]);
    let by_id: std::collections::HashMap<uuid::Uuid, SubTask> = current
        .into_iter()
        .map(|t| (t.id, t))
        .collect();

    let trace_id = uuid::Uuid::new_v4();
    for (task_id, agent_id) in assignment {
        let Some(mut task) = by_id.get(task_id).cloned() else {
            continue;
        };
        task.assigned_to = Some(*agent_id);
        let key = format!("task:{}", task.id);
        if let Err(e) = formation.blackboard.write(
            &key,
            serde_json::to_value(&task).unwrap_or_default(),
            trace_id,
            &formation.fuel,
        ) {
            tracing::warn!(task = %task.id, error = %e, "failed to rewrite assigned_to");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::{Formation, FormationMember};
    use springtale_cooperation::action::SubTask;
    use springtale_cooperation::cadence::IntentPattern;
    use springtale_cooperation::capability::CapabilityDecl;
    use springtale_cooperation::momentum::MomentumTier;
    use springtale_cooperation::types::{AgentHealth, FormationConstraints, FuelAmount};
    use uuid::Uuid;

    fn make_task(connector: &str) -> SubTask {
        SubTask {
            id: Uuid::new_v4(),
            target_connector: CapabilityDecl::new(connector),
            action_name: "send".into(),
            params: serde_json::json!({}),
            priority: 5,
            description: "task".into(),
            assigned_to: None,
        }
    }

    fn formation_with_members(caps: Vec<&str>) -> Formation {
        let mut members = Vec::new();
        for c in caps {
            let mut m = FormationMember::new(AgentId::new(), vec![CapabilityDecl::new(c)]);
            m.health = AgentHealth::Operational;
            members.push(m);
        }
        let mut f = Formation::new_disconnected(
            members,
            IntentPattern::Execute { plan_id: None },
            FormationConstraints {
                fuel_budget: FuelAmount(1_000_000),
                ..Default::default()
            },
        );
        // CBBA requires Fever tier — bump there for the test.
        f.momentum.tier = MomentumTier::Fever;
        f
    }

    #[tokio::test]
    async fn replan_clears_flag_when_no_tasks() {
        let mut f = formation_with_members(vec!["slack", "github"]);
        f.needs_replan = true;
        run(&mut f, None);
        assert!(!f.needs_replan, "no tasks → flag cleared");
    }

    #[tokio::test]
    async fn replan_clears_flag_when_no_operational_members() {
        let mut f = formation_with_members(vec!["slack"]);
        // Make the only member non-operational.
        f.members[0].health = AgentHealth::Dead { recoverable: false };
        f.needs_replan = true;
        run(&mut f, None);
        assert!(!f.needs_replan, "no operational members → flag cleared");
    }

    /// Plan §B3 cascade-trigger integration test: needs_replan + tasks +
    /// Fever tier + capable members → CBBA converges, assignments applied
    /// via blackboard rewrite, flag cleared.
    #[tokio::test]
    async fn cascade_replan_assigns_tasks_and_clears_flag() {
        let mut f = formation_with_members(vec!["slack", "github"]);
        let trace = Uuid::new_v4();
        let task_a = make_task("slack");
        let task_b = make_task("github");
        f.blackboard
            .write(
                &format!("task:{}", task_a.id),
                serde_json::to_value(&task_a).unwrap(),
                trace,
                &f.fuel,
            )
            .expect("post task A");
        f.blackboard
            .write(
                &format!("task:{}", task_b.id),
                serde_json::to_value(&task_b).unwrap(),
                trace,
                &f.fuel,
            )
            .expect("post task B");

        f.needs_replan = true;
        run(&mut f, None);
        assert!(!f.needs_replan, "converged → flag cleared");

        // After replan both tasks should have an assigned_to set on the
        // blackboard (CBBA picks the capable agent for each).
        let post = f.blackboard.scan_tasks(&[]);
        assert_eq!(post.len(), 2, "tasks remain on blackboard");
        for t in &post {
            assert!(
                t.assigned_to.is_some(),
                "task {} should have assigned_to after replan",
                t.id
            );
        }
    }

    #[tokio::test]
    async fn replan_unauthorized_below_fever_clears_flag() {
        let mut f = formation_with_members(vec!["slack"]);
        f.momentum.tier = MomentumTier::Hot;
        let trace = Uuid::new_v4();
        let task = make_task("slack");
        f.blackboard
            .write(
                &format!("task:{}", task.id),
                serde_json::to_value(&task).unwrap(),
                trace,
                &f.fuel,
            )
            .expect("post");
        f.needs_replan = true;
        run(&mut f, None);
        assert!(
            !f.needs_replan,
            "unauthorized at non-Fever tier → flag cleared (replan moot)"
        );
    }
}
