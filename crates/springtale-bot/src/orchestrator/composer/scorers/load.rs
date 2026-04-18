use super::super::trait_::{AgentCandidate, FormationSpec, ScorePlugin};

/// Prefer idle agents. Score = (1 - attention_load) clamped to `[0.0, 1.0]`.
pub struct LoadScorer {
    weight: f32,
}

impl LoadScorer {
    pub fn new(weight: f32) -> Self {
        Self { weight }
    }
}

impl ScorePlugin for LoadScorer {
    fn name(&self) -> &'static str {
        "load"
    }

    fn score(&self, candidate: &AgentCandidate, _spec: &FormationSpec) -> f32 {
        (1.0 - candidate.attention_load).clamp(0.0, 1.0)
    }

    fn weight(&self) -> f32 {
        self.weight
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cooperation::FormationConstraints;
    use crate::cooperation::cadence::{AgentId, IntentPattern};
    use crate::cooperation::momentum::MomentumTier;
    use crate::cooperation::types::{AgentHealth, AutonomyLevel};

    fn cand(load: f32) -> AgentCandidate {
        AgentCandidate {
            agent_id: AgentId::new(),
            capabilities: vec![],
            health: AgentHealth::Operational,
            momentum: MomentumTier::Warming,
            attention_load: load,
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
    fn idle_agent_scores_highest() {
        assert_eq!(LoadScorer::new(1.0).score(&cand(0.0), &spec()), 1.0);
    }

    #[test]
    fn busy_agent_scores_low() {
        let s = LoadScorer::new(1.0).score(&cand(0.9), &spec());
        assert!((s - 0.1).abs() < 1e-5, "expected ~0.1, got {s}");
    }

    #[test]
    fn over_saturated_agent_floors_at_zero() {
        assert_eq!(LoadScorer::new(1.0).score(&cand(1.5), &spec()), 0.0);
    }
}
