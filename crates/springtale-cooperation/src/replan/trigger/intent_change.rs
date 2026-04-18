//! Intent-change trigger — replan when the formation's intent shifts to a
//! variant that reshapes task priorities.
//!
//! Reconnoiter→Execute, Execute→Stabilize, Stabilize→Surge all reshape which
//! tasks are valuable. Dissolve is handled by lifecycle, not replan.

use crate::cadence::IntentPattern;

pub fn should_replan(previous: &IntentPattern, next: &IntentPattern) -> bool {
    !same_variant(previous, next) && !matches!(next, IntentPattern::Dissolve { .. })
}

fn same_variant(a: &IntentPattern, b: &IntentPattern) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn different_variant_triggers() {
        let prev = IntentPattern::Reconnoiter {
            target: "queue".into(),
        };
        let next = IntentPattern::Execute { plan_id: None };
        assert!(should_replan(&prev, &next));
    }

    #[test]
    fn same_variant_does_not_trigger() {
        let prev = IntentPattern::Execute { plan_id: None };
        let next = IntentPattern::Execute {
            plan_id: Some("new".into()),
        };
        assert!(!should_replan(&prev, &next));
    }

    #[test]
    fn dissolve_does_not_trigger_replan() {
        let prev = IntentPattern::Execute { plan_id: None };
        let next = IntentPattern::Dissolve {
            reason: "shutdown".into(),
        };
        assert!(!should_replan(&prev, &next));
    }
}
