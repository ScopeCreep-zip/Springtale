use crate::cooperation::formation::Formation;
use springtale_cooperation::cadence::ActionSummary;

use super::super::types::InterventionError;

/// Apply an `EscalateToUser`. Logs a high-severity event; the UI layer
/// surfaces the summary through existing diagnostic channels.
pub fn apply(
    formation: &Formation,
    summary: ActionSummary,
) -> Result<(), InterventionError> {
    tracing::error!(
        formation_id = %formation.id.0,
        summary = %summary,
        "intervention: escalate to user"
    );
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::cadence::IntentPattern;
    use crate::cooperation::formation::FormationMember;
    use crate::cooperation::types::FormationConstraints;
    use springtale_cooperation::cadence::AgentId;

    fn formation() -> Formation {
        Formation::new_disconnected(
            vec![FormationMember::new(AgentId::new(), vec!["github".into()])],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
        )
    }

    #[test]
    fn returns_ok_and_leaves_formation_untouched() {
        let f = formation();
        let intent_before = matches!(f.intent, IntentPattern::Execute { .. });
        apply(&f, "something".into()).unwrap();
        assert!(intent_before);
    }
}
