use std::collections::HashSet;

use crate::cooperation::cadence::AgentId;

use super::super::trait_::{AgentCandidate, FormationSpec, ScorePlugin};

/// Prefer candidates who have successfully worked together before. The
/// scorer is seeded with a history set of agent ids; candidates in the set
/// score 1.0, otherwise 0.5 (no data, no penalty).
pub struct AffinityScorer {
    weight: f32,
    history: HashSet<AgentId>,
}

impl AffinityScorer {
    pub fn new(weight: f32, history: HashSet<AgentId>) -> Self {
        Self { weight, history }
    }

    pub fn empty(weight: f32) -> Self {
        Self::new(weight, HashSet::new())
    }
}

impl ScorePlugin for AffinityScorer {
    fn name(&self) -> &'static str {
        "affinity"
    }

    fn score(&self, candidate: &AgentCandidate, _spec: &FormationSpec) -> f32 {
        if self.history.contains(&candidate.agent_id) {
            1.0
        } else {
            0.5
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
    use crate::cooperation::cadence::IntentPattern;
    use crate::cooperation::momentum::MomentumTier;
    use crate::cooperation::types::{AgentHealth, AutonomyLevel};

    fn cand(id: AgentId) -> AgentCandidate {
        AgentCandidate {
            agent_id: id,
            capabilities: vec![],
            health: AgentHealth::Operational,
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
    fn agent_in_history_scores_one() {
        let id = AgentId::new();
        let mut hist = HashSet::new();
        hist.insert(id);
        let s = AffinityScorer::new(1.0, hist);
        assert_eq!(s.score(&cand(id), &spec()), 1.0);
    }

    #[test]
    fn agent_without_history_scores_midpoint() {
        let s = AffinityScorer::empty(1.0);
        assert_eq!(s.score(&cand(AgentId::new()), &spec()), 0.5);
    }
}
