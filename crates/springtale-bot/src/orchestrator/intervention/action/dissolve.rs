use crate::cooperation::cadence::IntentPattern;
use crate::cooperation::formation::Formation;
use springtale_cooperation::cadence::DissolveReason;

use super::super::types::InterventionError;

/// Apply a `ForcedDissolve`. Rewrites intent to `Dissolve` so the runtime
/// lifecycle layer removes the formation on its next sweep and every watcher
/// observes the shutdown reason.
pub fn apply(formation: &mut Formation, reason: DissolveReason) -> Result<(), InterventionError> {
    let logged_reason = reason.0.clone();
    crate::orchestrator::intent::apply_intent(formation, IntentPattern::Dissolve { reason });
    tracing::warn!(
        formation_id = %formation.id.0,
        reason = %logged_reason,
        "intervention: forced dissolve"
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
    fn sets_dissolve_intent_with_reason() {
        let mut f = formation();
        apply(&mut f, "cascade".into()).unwrap();
        let IntentPattern::Dissolve { reason } = &f.intent else {
            panic!("expected Dissolve");
        };
        assert_eq!(reason.as_ref(), "cascade");
    }
}
