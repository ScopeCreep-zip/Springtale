use super::super::trait_::{AgentCandidate, FilterPlugin, FormationSpec};

/// Rejects candidates whose current attention load exceeds a hard cap —
/// oversaturated agents will starve any new formation they join. `LOAD_CAP`
/// defaults to 0.9; tune via a future settings surface when operations
/// demand it.
pub struct AttentionFilter {
    pub load_cap: f32,
}

impl AttentionFilter {
    pub fn new(load_cap: f32) -> Self {
        Self { load_cap }
    }
}

impl Default for AttentionFilter {
    fn default() -> Self {
        Self { load_cap: 0.9 }
    }
}

impl FilterPlugin for AttentionFilter {
    fn name(&self) -> &'static str {
        "attention"
    }

    fn accept(&self, candidate: &AgentCandidate, _spec: &FormationSpec) -> bool {
        candidate.attention_load < self.load_cap
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
    fn under_cap_admitted() {
        assert!(AttentionFilter::default().accept(&cand(0.4), &spec()));
    }

    #[test]
    fn at_cap_rejected() {
        assert!(!AttentionFilter::default().accept(&cand(0.95), &spec()));
    }

    #[test]
    fn configurable_cap() {
        let strict = AttentionFilter::new(0.5);
        assert!(!strict.accept(&cand(0.6), &spec()));
    }
}
