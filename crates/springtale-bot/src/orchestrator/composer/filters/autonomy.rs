use crate::cooperation::types::AutonomyLevel;

use super::super::trait_::{AgentCandidate, FilterPlugin, FormationSpec};

/// Rejects candidates whose autonomy level is `Observe` — an observer cannot
/// fulfill formation tasks. All other levels (Suggest / ActWithApproval /
/// ActAutonomously) are admissible; they just behave differently at tick time.
pub struct AutonomyFilter;

impl FilterPlugin for AutonomyFilter {
    fn name(&self) -> &'static str {
        "autonomy"
    }

    fn accept(&self, candidate: &AgentCandidate, _spec: &FormationSpec) -> bool {
        candidate.autonomy_level != AutonomyLevel::Observe
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::FormationConstraints;
    use crate::cooperation::cadence::{AgentId, IntentPattern};
    use crate::cooperation::momentum::MomentumTier;
    use crate::cooperation::types::AgentHealth;

    fn cand(level: AutonomyLevel) -> AgentCandidate {
        AgentCandidate {
            agent_id: AgentId::new(),
            capabilities: vec![],
            health: AgentHealth::Operational,
            momentum: MomentumTier::Warming,
            attention_load: 0.0,
            autonomy_level: level,
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
    fn observe_rejected() {
        assert!(!AutonomyFilter.accept(&cand(AutonomyLevel::Observe), &spec()));
    }

    #[test]
    fn autonomous_admitted() {
        assert!(AutonomyFilter.accept(&cand(AutonomyLevel::ActAutonomously), &spec()));
    }

    #[test]
    fn suggest_admitted() {
        assert!(AutonomyFilter.accept(&cand(AutonomyLevel::Suggest), &spec()));
    }

    #[test]
    fn act_with_approval_admitted() {
        assert!(AutonomyFilter.accept(&cand(AutonomyLevel::ActWithApproval), &spec()));
    }
}
