use crate::cooperation::cadence::IntentPattern;
use crate::cooperation::formation::Formation;

use super::super::types::InterventionError;

/// Apply a `ChangeIntent` intervention. Routes through the §3.2
/// `apply_intent` chokepoint so the momentum FSM observes the change and
/// the context watch channel rebroadcasts — same path as user commands
/// and consensus-approved intent changes.
pub fn apply(formation: &mut Formation, intent: IntentPattern) -> Result<(), InterventionError> {
    crate::orchestrator::intent::apply_intent(formation, intent);
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
        Formation::new_disconnected(
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
