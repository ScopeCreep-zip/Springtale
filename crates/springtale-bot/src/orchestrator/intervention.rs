//! Intervention — the general's rally. Reactive and exceptional.
//!
//! Per COOPERATION.pdf §3.4:
//! Game source: Total War general rally, Left 4 Dead Director,
//! Patapon rhythm switch.
//!
//! "Only when cooperation self-recovery (§15) fails does the
//! formation escalate to orchestrator::intervention."
//!
//! Intervention is the orchestrator's last resort. The cooperation
//! module handles everything it can through self-rally (§15),
//! awareness (§8), and recovery (§18). Only when those fail —
//! tokens consumed, Cold momentum, multiple agents failing —
//! does the orchestrator step in.

use super::fuel::FuelBudget;
use crate::cooperation::cadence::IntentPattern;

/// Orchestrator interventions — reactive, not proactive.
///
/// From COOPERATION.pdf §3.4:
/// ```text
/// pub enum Intervention {
///     ChangeIntent(IntentPattern),      // Patapon: switch rhythm
///     InjectFuel(FuelBudget),           // L4D: health kit spawn
///     ForcedDissolve { reason: String },
///     EscalateToUser { summary: ActionSummary },
/// }
/// ```
pub enum Intervention {
    /// Switch the formation's intent pattern.
    /// Patapon: switch from attack rhythm to defend rhythm.
    ChangeIntent(IntentPattern),

    /// Inject additional fuel into the formation.
    /// L4D Director: spawning health kits when team is struggling.
    InjectFuel(FuelBudget),

    /// Force-dissolve a stuck formation.
    /// Emergency: formation is no longer viable and self-rally failed.
    ForcedDissolve { reason: String },

    /// Escalate to the user for a decision.
    /// When the orchestrator itself can't resolve the situation.
    EscalateToUser { summary: String },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_intervention_variants() {
        let _change = Intervention::ChangeIntent(IntentPattern::Stabilize {
            reason: "cascade detected".into(),
        });
        let _fuel = Intervention::InjectFuel(FuelBudget::new(5000));
        let _dissolve = Intervention::ForcedDissolve {
            reason: "all agents incapacitated".into(),
        };
        let _escalate = Intervention::EscalateToUser {
            summary: "formation stuck in Cold momentum for 5 minutes".into(),
        };
    }
}
