use crate::cooperation::cadence::IntentPattern;

use super::super::trait_::InterventionEvaluator;
use super::super::types::{Intervention, InterventionSignals};
use super::thresholds::InterventionThresholds;

/// Rule-based evaluator. Rules check in priority order — the first matching
/// rule wins. Rules listed from most-severe to least-severe:
///
/// 1. Terminal incapacitation → `ForcedDissolve`
/// 2. CBBA stalled + no rally tokens → `ForcedDissolve`
/// 3. Cascade + zero rally → `ForcedDissolve`
/// 4. Cold-stuck past `cold_escalate_ticks` → `EscalateToUser`
/// 5. Cascade hits ≥ `cascade_stabilize` → `ChangeIntent(Stabilize)`
/// 6. Otherwise no intervention.
pub struct RuleBasedEvaluator {
    pub thresholds: InterventionThresholds,
}

impl RuleBasedEvaluator {
    pub fn new(thresholds: InterventionThresholds) -> Self {
        Self { thresholds }
    }
}

impl Default for RuleBasedEvaluator {
    fn default() -> Self {
        Self::new(InterventionThresholds::default())
    }
}

impl InterventionEvaluator for RuleBasedEvaluator {
    fn evaluate(&self, signals: &InterventionSignals) -> Option<Intervention> {
        let t = &self.thresholds;

        if t.is_terminal_incapacitation(signals.incapacitated_agents, signals.operational_count) {
            return Some(Intervention::ForcedDissolve {
                reason: format!(
                    "{} of {} members incapacitated",
                    signals.incapacitated_agents, signals.operational_count
                ),
            });
        }

        if signals.cbba_stalled && signals.rally_tokens <= t.rally_dissolve_floor {
            return Some(Intervention::ForcedDissolve {
                reason: "CBBA stalled with rally tokens exhausted".to_owned(),
            });
        }

        if signals.cascade_hits >= t.cascade_stabilize
            && signals.rally_tokens <= t.rally_dissolve_floor
        {
            return Some(Intervention::ForcedDissolve {
                reason: "cascade persists past rally budget".to_owned(),
            });
        }

        if signals.cold_duration_ticks >= t.cold_escalate_ticks {
            return Some(Intervention::EscalateToUser {
                summary: format!(
                    "formation stuck in Cold for {} ticks",
                    signals.cold_duration_ticks
                ),
            });
        }

        if signals.cascade_hits >= t.cascade_stabilize {
            return Some(Intervention::ChangeIntent(IntentPattern::Stabilize {
                reason: "cascade detected — falling back to Stabilize intent".to_owned(),
            }));
        }

        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::super::types::Intervention;
    use super::*;

    fn evaluator() -> RuleBasedEvaluator {
        RuleBasedEvaluator::default()
    }

    #[test]
    fn terminal_incapacitation_dissolves() {
        let s = InterventionSignals {
            incapacitated_agents: 3,
            operational_count: 4,
            ..Default::default()
        };
        let out = evaluator().evaluate(&s).unwrap();
        assert!(matches!(out, Intervention::ForcedDissolve { .. }));
    }

    #[test]
    fn cbba_stalled_with_no_rally_dissolves() {
        let s = InterventionSignals {
            cbba_stalled: true,
            rally_tokens: 0,
            operational_count: 5,
            incapacitated_agents: 0,
            ..Default::default()
        };
        let out = evaluator().evaluate(&s).unwrap();
        assert!(matches!(out, Intervention::ForcedDissolve { .. }));
    }

    #[test]
    fn cascade_with_rally_tokens_downshifts_to_stabilize() {
        let s = InterventionSignals {
            cascade_hits: 3,
            rally_tokens: 2,
            operational_count: 5,
            ..Default::default()
        };
        let out = evaluator().evaluate(&s).unwrap();
        assert!(matches!(out, Intervention::ChangeIntent(_)));
    }

    #[test]
    fn cold_duration_escalates_to_user() {
        let s = InterventionSignals {
            cold_duration_ticks: 700,
            operational_count: 4,
            ..Default::default()
        };
        let out = evaluator().evaluate(&s).unwrap();
        assert!(matches!(out, Intervention::EscalateToUser { .. }));
    }

    #[test]
    fn healthy_formation_yields_no_intervention() {
        let s = InterventionSignals {
            operational_count: 4,
            rally_tokens: 3,
            ..Default::default()
        };
        assert!(evaluator().evaluate(&s).is_none());
    }
}
