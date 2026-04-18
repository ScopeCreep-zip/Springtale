use thiserror::Error;

use super::super::fuel::FuelBudget;
use crate::cooperation::cadence::IntentPattern;

/// Orchestrator interventions — reactive, not proactive.
///
/// Matches COOPERATION.md §3.4 taxonomy. The evaluator picks one based on
/// which cooperation layer broke; actions in `action/` each execute one
/// variant against a Formation.
pub enum Intervention {
    /// Patapon: switch rhythm. Replaces the formation's intent.
    ChangeIntent(IntentPattern),
    /// L4D Director: drop a health kit. Refills the fuel budget.
    InjectFuel(FuelBudget),
    /// Unrecoverable — tear down the formation with a reason.
    ForcedDissolve { reason: String },
    /// Orchestrator can't decide — raise to the human user.
    EscalateToUser { summary: String },
}

/// Inputs the evaluator considers when deciding whether to intervene.
///
/// Every field is a signal the cooperation layer already emits; the
/// evaluator is pure over these fields so it can be unit-tested without
/// wiring a Formation.
#[derive(Debug, Clone, Copy, Default)]
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
}

#[derive(Debug, Error)]
pub enum InterventionError {
    #[error("intervention not applicable: {0}")]
    NotApplicable(String),
    #[error("action failed: {0}")]
    ActionFailed(String),
}
