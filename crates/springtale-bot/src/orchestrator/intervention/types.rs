use thiserror::Error;

use super::super::fuel::FuelBudget;
use crate::cooperation::cadence::IntentPattern;
use springtale_cooperation::cadence::{ActionSummary, DissolveReason};

/// Orchestrator interventions — reactive, not proactive.
///
/// Matches COOPERATION.md §3.4 taxonomy. The evaluator picks one based on
/// which cooperation layer broke; actions in `action/` each execute one
/// variant against a Formation.
#[derive(Debug, Clone)]
pub enum Intervention {
    /// Patapon: switch rhythm. Replaces the formation's intent.
    ChangeIntent(IntentPattern),
    /// L4D Director: drop a health kit. Refills the fuel budget.
    InjectFuel(FuelBudget),
    /// Unrecoverable — tear down the formation with a reason.
    ForcedDissolve { reason: DissolveReason },
    /// Orchestrator can't decide — raise to the human user.
    EscalateToUser { summary: ActionSummary },
}

/// Inputs the evaluator considers when deciding whether to intervene.
///
/// Every field is a signal the cooperation layer already emits; the
/// evaluator is pure over these fields so it can be unit-tested without
/// wiring a Formation.
#[derive(Debug, Clone, Default)]
pub struct InterventionSignals {
    /// Consecutive cascade-trigger hits in the current window.
    pub cascade_hits: u32,
    /// Remaining rally tokens.
    pub rally_tokens: u32,
    /// Whether the most recent CBBA replan reported `Stalled`.
    pub cbba_stalled: bool,
    /// Number of agents in terminal health states.
    pub incapacitated_agents: u32,
    /// Total operational members at tick time.
    pub operational_count: u32,
    /// Ticks spent in `Cold` without recovery.
    pub cold_duration_ticks: u32,
    /// Reason set by `tick_steps/supervision.rs` (B10) when the
    /// supervisor returns `SupervisionAction::Escalate`. The L6
    /// evaluator (B1) reads this and routes to `EscalateToUser` so
    /// supervisor escalations don't silently drop.
    pub escalation_reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum InterventionError {
    #[error("intervention not applicable: {0}")]
    NotApplicable(String),
    #[error("action failed: {0}")]
    ActionFailed(String),
}
