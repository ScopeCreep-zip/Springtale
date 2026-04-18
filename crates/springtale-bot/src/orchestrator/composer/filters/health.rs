use crate::cooperation::types::AgentHealth;

use super::super::trait_::{AgentCandidate, FilterPlugin, FormationSpec};

/// Rejects candidates in terminal health states. `Healthy` and `Degraded`
/// are admissible (degraded agents can still contribute); `Incapacitated`
/// and `Dead` are not.
pub struct HealthFilter;

impl FilterPlugin for HealthFilter {
    fn name(&self) -> &'static str {
        "health"
    }

    fn accept(&self, candidate: &AgentCandidate, _spec: &FormationSpec) -> bool {
        !matches!(
            candidate.health,
            AgentHealth::Incapacitated | AgentHealth::Dead { .. }
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::FormationConstraints;
    use crate::cooperation::cadence::{AgentId, IntentPattern};
    use crate::cooperation::momentum::MomentumTier;
    use crate::cooperation::types::AutonomyLevel;

    fn cand_at(health: AgentHealth) -> AgentCandidate {
        AgentCandidate {
            agent_id: AgentId::new(),
            capabilities: vec![],
            health,
            momentum: MomentumTier::Warming,
            attention_load: 0.0,
            autonomy_level: AutonomyLevel::ActAutonomously,
        }
    }

    fn spec() -> FormationSpec {
        FormationSpec {
            required_capabilities: vec![],
            intent: IntentPattern::Execute { plan_id: None },
            constraints: FormationConstraints::default(),
            min_members: 1,
            max_members: 3,
        }
    }

    #[test]
    fn operational_admitted() {
        assert!(HealthFilter.accept(&cand_at(AgentHealth::Operational), &spec()));
    }

    #[test]
    fn degraded_admitted() {
        assert!(HealthFilter.accept(
            &cand_at(AgentHealth::Degraded { recovery_count: 1 }),
            &spec()
        ));
    }

    #[test]
    fn incapacitated_rejected() {
        assert!(!HealthFilter.accept(&cand_at(AgentHealth::Incapacitated), &spec()));
    }

    #[test]
    fn dead_rejected() {
        assert!(!HealthFilter.accept(
            &cand_at(AgentHealth::Dead { recoverable: false }),
            &spec()
        ));
    }
}
