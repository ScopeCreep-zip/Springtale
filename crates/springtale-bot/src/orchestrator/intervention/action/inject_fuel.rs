use crate::cooperation::formation::Formation;
use crate::orchestrator::fuel::FuelBudget;

use super::super::types::InterventionError;

/// Apply an `InjectFuel` intervention — add `new_budget.initial()` worth of
/// fuel back into the formation's active budget without discarding the
/// existing consumption audit trail.
pub fn apply(formation: &mut Formation, new_budget: &FuelBudget) -> Result<(), InterventionError> {
    let topup = new_budget.initial();
    formation.fuel.replenish(topup);
    tracing::info!(
        formation_id = %formation.id.0,
        topup,
        remaining = formation.fuel.remaining(),
        "intervention: fuel injected"
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

    fn formation(fuel: u64) -> Formation {
        let constraints = FormationConstraints {
            fuel_budget: springtale_cooperation::FuelAmount(fuel),
            ..Default::default()
        };
        Formation::new_disconnected(
            vec![FormationMember::new(AgentId::new(), vec!["github".into()])],
            IntentPattern::Execute { plan_id: None },
            constraints,
        )
    }

    #[test]
    fn replenishes_fuel_additively() {
        let mut f = formation(500);
        let _ = f.fuel.consume(200).unwrap();
        let before = f.fuel.remaining();
        apply(&mut f, &FuelBudget::new(1000)).unwrap();
        assert_eq!(f.fuel.remaining(), before + 1000);
    }
}
