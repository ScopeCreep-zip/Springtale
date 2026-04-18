//! Per-agent behavioral loop — the game AI engine brain.
//!
//! Per Spring engine CMobileCAI::SlowUpdate():
//! - If queue empty → AutoGenerateTarget (scan for work)
//! - Else → Execute (dispatch current task)
//!
//! Per RimWorld ThinkTree:
//! - Constant tree: check emergency interrupts
//! - Primary tree: scan for work by priority
//!
//! Per 0 A.D. stance system:
//! - observe = holdfire + holdpos (just watch)
//! - suggest = returnfire + holdpos (report what WOULD be done)
//! - approve = fireatwill + maneuver (act with approval)
//! - autonomous = fireatwill + roam (full autonomy)
//!
//! This module defines the decision logic. The actual execution
//! (connector dispatch) happens in the bot crate via task_dispatch.rs
//! → dispatch_action(). This module is pure decision-making, no I/O.

use crate::action::SubTask;
use crate::action_state::{ActionState, ActiveTask};
use crate::cadence::{ActionDescriptor, AgentId};
use crate::capability::CapabilityDecl;
use crate::types::AutonomyLevel;
use crate::utility::measure::{Measure, WeightedSum};
use crate::utility::picker::{Highest, Picker};

/// Result of one agent's tick — feeds back into the formation tick pipeline.
pub struct AgentTickResult {
    /// Which agent this result is for.
    pub agent_id: AgentId,
    /// What action was taken this tick (for TickReport.action_taken).
    /// None means the agent was idle or in observe/suggest mode.
    pub action: Option<ActionDescriptor>,
    /// How well aligned the agent's action was with formation intent (0.0-1.0).
    pub alignment: f32,
    /// Agents interfered with (if any).
    pub interference_with: Vec<AgentId>,
    /// Whether a task was claimed this tick.
    pub task_claimed: Option<SubTask>,
    /// Whether a task completed this tick.
    pub task_completed: bool,
}

/// Decide what an agent should do this tick.
///
/// This is the DECIDE step of the Perceive→Decide→Act→Report loop.
/// It does NOT execute the action — it returns what SHOULD be done.
/// The caller (bot event loop) handles actual execution via dispatch.
///
/// Per the autonomy level (stance system):
/// - "observe": just report current state, no action selection
/// - "suggest": scan blackboard, report what WOULD be done, don't claim
/// - "act-with-approval": scan, claim, but mark as Requested (not Executing)
/// - "act-autonomously": scan, claim, mark as Executing
pub fn decide_agent_tick(
    agent_id: AgentId,
    capabilities: &[CapabilityDecl],
    active_task: &Option<ActiveTask>,
    available_tasks: &[SubTask],
    attention_load: f32,
    autonomy_level: AutonomyLevel,
    current_tick: u64,
) -> AgentDecision {
    // Per 0 A.D.: Observe = holdfire + holdpos (AoE "No Attack")
    if autonomy_level == AutonomyLevel::Observe {
        return AgentDecision::Idle;
    }

    // If agent has an active task that isn't terminal, continue it
    if let Some(task) = active_task {
        // Timeout detection: if task has been active too long, cancel it
        // Per Spring engine: auto-generated attacks expire after 5 seconds
        let ticks_active = task.ticks_elapsed(current_tick);
        if task.state.is_active() && ticks_active > 300 {
            return AgentDecision::HandleCancellation;
        }

        match &task.state {
            ActionState::Executing => {
                return AgentDecision::ContinueExecuting;
            }
            ActionState::Requested => {
                // If autonomous (AoE "Aggressive"), promote to Executing
                if autonomy_level == AutonomyLevel::ActAutonomously {
                    return AgentDecision::PromoteToExecuting;
                }
                // Otherwise wait for approval (AoE "Defensive" / "Stand Ground")
                return AgentDecision::WaitForApproval;
            }
            ActionState::Cancelled => {
                return AgentDecision::HandleCancellation;
            }
            ActionState::Init => {
                return AgentDecision::AdvanceToRequested;
            }
            _ => {
                // Terminal — fall through to scan for new work
            }
        }
    }

    // No active task (or active task is terminal) — scan for work
    if autonomy_level == AutonomyLevel::Suggest {
        // AoE "Stand Ground": report what WOULD be done, don't claim
        if let Some(best) = score_and_pick(agent_id, capabilities, available_tasks, attention_load) {
            return AgentDecision::Suggest(best.clone());
        }
        return AgentDecision::Idle;
    }

    // Scan blackboard for matching work
    if let Some(best) = score_and_pick(agent_id, capabilities, available_tasks, attention_load) {
        return AgentDecision::ClaimTask {
            task: best.clone(),
            auto_execute: autonomy_level == AutonomyLevel::ActAutonomously,
        };
    }

    AgentDecision::Idle
}

/// The decision the agent makes this tick.
pub enum AgentDecision {
    /// Nothing to do — agent is idle.
    Idle,
    /// Continue executing current active task.
    ContinueExecuting,
    /// Advance newly claimed task from Init to Requested.
    AdvanceToRequested,
    /// Promote Requested task to Executing (autonomous mode).
    PromoteToExecuting,
    /// Wait for user/consensus approval before executing.
    WaitForApproval,
    /// Handle a cancelled task — clean up and report.
    HandleCancellation,
    /// Suggest a task without claiming it (suggest mode).
    Suggest(SubTask),
    /// Claim a task from the blackboard.
    ClaimTask {
        task: SubTask,
        /// If true, immediately start executing (autonomous mode).
        /// If false, enter Requested state and wait for approval.
        auto_execute: bool,
    },
}

/// Score available tasks and pick the best one for this agent.
///
/// Per RimWorld's WorkGiver_Scanner: filter by capability, sort by
/// priority, then score by utility (attention load, assignment hint).
fn score_and_pick<'a>(
    agent_id: AgentId,
    capabilities: &[CapabilityDecl],
    available_tasks: &'a [SubTask],
    attention_load: f32,
) -> Option<&'a SubTask> {
    if available_tasks.is_empty() {
        return None;
    }

    let measure = WeightedSum;
    let picker = Highest;

    let scores: Vec<(usize, f32)> = available_tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| capabilities.iter().any(|c| c == &task.target_connector))
        .map(|(i, task)| {
            // Priority score: priority 1 = 0.9, priority 10 = 0.0
            let priority_score = 1.0 - (task.priority as f32 / 10.0).min(1.0);

            // Assignment hint bonus: if task is specifically assigned to this agent
            let assignment_bonus = if task.assigned_to == Some(agent_id) { 0.2 } else { 0.0 };

            // Free capacity: idle agents score higher
            let capacity_score = 1.0 - attention_load;

            let score = measure.calculate(&[
                (priority_score, 0.4),
                (assignment_bonus, 0.1),
                (capacity_score, 0.3),
                (1.0, 0.2), // baseline — always somewhat willing to take work
            ]);

            (i, score)
        })
        .collect();

    picker
        .pick(&scores)
        .map(|idx| &available_tasks[idx])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_task(connector: &str, priority: u8) -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: CapabilityDecl::new(connector),
            action_name: "test_action".to_owned(),
            params: serde_json::json!({}),
            priority,
            assigned_to: None,
            description: "test".to_owned(),
        }
    }

    #[test]
    fn test_observe_always_idle() {
        let decision = decide_agent_tick(
            AgentId::new(),
            &[CapabilityDecl::new("github")],
            &None,
            &[make_task("github", 1)],
            0.0,
            AutonomyLevel::Observe,
            1,
        );
        assert!(matches!(decision, AgentDecision::Idle));
    }

    #[test]
    fn test_suggest_reports_without_claiming() {
        let decision = decide_agent_tick(
            AgentId::new(),
            &[CapabilityDecl::new("github")],
            &None,
            &[make_task("github", 1)],
            0.0,
            AutonomyLevel::Suggest,
            1,
        );
        assert!(matches!(decision, AgentDecision::Suggest(_)));
    }

    #[test]
    fn test_autonomous_claims_task() {
        let decision = decide_agent_tick(
            AgentId::new(),
            &[CapabilityDecl::new("github")],
            &None,
            &[make_task("github", 1)],
            0.0,
            AutonomyLevel::ActAutonomously,
            1,
        );
        assert!(matches!(
            decision,
            AgentDecision::ClaimTask { auto_execute: true, .. }
        ));
    }

    #[test]
    fn test_approve_claims_without_auto_execute() {
        let decision = decide_agent_tick(
            AgentId::new(),
            &[CapabilityDecl::new("github")],
            &None,
            &[make_task("github", 1)],
            0.0,
            AutonomyLevel::ActWithApproval,
            1,
        );
        assert!(matches!(
            decision,
            AgentDecision::ClaimTask { auto_execute: false, .. }
        ));
    }

    #[test]
    fn test_no_matching_tasks_idle() {
        let decision = decide_agent_tick(
            AgentId::new(),
            &[CapabilityDecl::new("slack")],
            &None,
            &[make_task("github", 1)],
            0.0,
            AutonomyLevel::ActAutonomously,
            1,
        );
        assert!(matches!(decision, AgentDecision::Idle));
    }

    #[test]
    fn test_continue_executing_active_task() {
        let active = ActiveTask {
            task: make_task("github", 1),
            state: ActionState::Executing,
            claimed_at: std::time::Instant::now(),
            claimed_tick: 0,
            claimed_by: AgentId::new(),
        };

        let decision = decide_agent_tick(
            AgentId::new(),
            &[CapabilityDecl::new("github")],
            &Some(active),
            &[],
            0.5,
            AutonomyLevel::ActAutonomously,
            5,
        );
        assert!(matches!(decision, AgentDecision::ContinueExecuting));
    }

    #[test]
    fn test_higher_priority_task_scored_first() {
        let tasks = vec![make_task("github", 5), make_task("github", 1)];
        let result = score_and_pick(AgentId::new(), &[CapabilityDecl::new("github")], &tasks, 0.0);
        assert!(result.is_some());
        // Priority 1 should score higher than priority 5
        assert_eq!(result.as_ref().map(|t| t.priority), Some(1));
    }
}
