use super::super::trait_::{AgentCandidate, FormationSpec, ScorePlugin};

/// Score candidates by the fraction of `required_capabilities` they cover.
/// Unlike the `CapabilityFilter` (which is a hard any-overlap gate), this is
/// a soft measure: an agent carrying *all* required capabilities scores
/// higher than one carrying *one*.
pub struct SkillFitScorer {
    weight: f32,
}

impl SkillFitScorer {
    pub fn new(weight: f32) -> Self {
        Self { weight }
    }
}

impl ScorePlugin for SkillFitScorer {
    fn name(&self) -> &'static str {
        "skill_fit"
    }

    fn score(&self, candidate: &AgentCandidate, spec: &FormationSpec) -> f32 {
        if spec.required_capabilities.is_empty() {
            return 0.5; // no signal; midpoint
        }
        let hit = spec
            .required_capabilities
            .iter()
            .filter(|req| candidate.capabilities.iter().any(|c| c == *req))
            .count();
        hit as f32 / spec.required_capabilities.len() as f32
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

    fn cand(caps: &[&str]) -> AgentCandidate {
        AgentCandidate {
            agent_id: AgentId::new(),
            capabilities: caps.iter().map(|s| crate::cooperation::capability::CapabilityDecl::new(*s)).collect(),
            health: AgentHealth::Operational,
            momentum: MomentumTier::Warming,
            attention_load: 0.0,
            autonomy_level: AutonomyLevel::ActAutonomously,
        }
    }

    fn spec(req: &[&str]) -> FormationSpec {
        FormationSpec {
            required_capabilities: req.iter().map(|s| crate::cooperation::capability::CapabilityDecl::new(*s)).collect(),
            intent: IntentPattern::Execute { plan_id: None },
            constraints: FormationConstraints::default(),
            min_members: 1,
            max_members: 3,
        }
    }

    #[test]
    fn full_coverage_scores_one() {
        assert_eq!(
            SkillFitScorer::new(1.0).score(&cand(&["a", "b"]), &spec(&["a", "b"])),
            1.0
        );
    }

    #[test]
    fn partial_coverage_scores_proportional() {
        assert_eq!(
            SkillFitScorer::new(1.0).score(&cand(&["a"]), &spec(&["a", "b"])),
            0.5
        );
    }

    #[test]
    fn no_coverage_scores_zero() {
        assert_eq!(
            SkillFitScorer::new(1.0).score(&cand(&["c"]), &spec(&["a", "b"])),
            0.0
        );
    }

    #[test]
    fn empty_spec_returns_midpoint() {
        assert_eq!(
            SkillFitScorer::new(1.0).score(&cand(&["a"]), &spec(&[])),
            0.5
        );
    }
}
