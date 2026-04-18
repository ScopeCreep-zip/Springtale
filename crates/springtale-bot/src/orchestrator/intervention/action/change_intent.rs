use crate::cooperation::cadence::IntentPattern;
use crate::cooperation::formation::Formation;

use super::super::types::InterventionError;

/// Apply a `ChangeIntent` intervention. Rewrites the formation's intent and
/// pushes the new context to all members over the watch channel.
pub fn apply(formation: &mut Formation, intent: IntentPattern) -> Result<(), InterventionError> {
    formation.intent = intent;
    formation.broadcast_context();
    tracing::info!(
        formation_id = %formation.id.0,
        "intervention: intent changed"
    );
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::formation::FormationMember;
    use crate::cooperation::types::FormationConstraints;
    use springtale_cooperation::cadence::AgentId;

    fn formation() -> Formation {
        Formation::new(
            vec![FormationMember::new(AgentId::new(), vec!["github".into()])],
            IntentPattern::Execute { plan_id: None },
            FormationConstraints::default(),
        )
    }

    #[test]
    fn apply_swaps_intent() {
        let mut f = formation();
        apply(
            &mut f,
            IntentPattern::Stabilize {
                reason: "cooling".into(),
            },
        )
        .unwrap();
        assert!(matches!(f.intent, IntentPattern::Stabilize { .. }));
    }
}
