use crate::cooperation::momentum::MomentumTier;

use super::super::trait_::{AgentCandidate, FormationSpec, ScorePlugin};

/// Prefer agents at higher momentum tiers — they've already built coherence
/// and can access more cooperative primitives (§7).
pub struct MomentumScorer {
    weight: f32,
}

impl MomentumScorer {
    pub fn new(weight: f32) -> Self {
        Self { weight }
    }
}

impl ScorePlugin for MomentumScorer {
    fn name(&self) -> &'static str {
        "momentum"
    }

    fn score(&self, candidate: &AgentCandidate, _spec: &FormationSpec) -> f32 {
        match candidate.momentum {
            MomentumTier::Cold => 0.2,
            MomentumTier::Warming => 0.5,
            MomentumTier::Hot => 0.85,
            MomentumTier::Fever => 1.0,
        }
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
    use crate::cooperation::types::{AgentHealth, AutonomyLevel};

    fn cand(tier: MomentumTier) -> AgentCandidate {
        AgentCandidate {
            agent_id: AgentId::new(),
            capabilities: vec![],
            health: AgentHealth::Operational,
            momentum: tier,
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
    fn higher_tier_scores_higher() {
        let s = MomentumScorer::new(1.0);
        assert!(
            s.score(&cand(MomentumTier::Fever), &spec())
                > s.score(&cand(MomentumTier::Warming), &spec())
        );
        assert!(
            s.score(&cand(MomentumTier::Warming), &spec())
                > s.score(&cand(MomentumTier::Cold), &spec())
        );
    }

    #[test]
    fn all_scores_in_unit_range() {
        let s = MomentumScorer::new(1.0);
        for t in [
            MomentumTier::Cold,
            MomentumTier::Warming,
            MomentumTier::Hot,
            MomentumTier::Fever,
        ] {
            let v = s.score(&cand(t), &spec());
            assert!((0.0..=1.0).contains(&v));
        }
    }
}
