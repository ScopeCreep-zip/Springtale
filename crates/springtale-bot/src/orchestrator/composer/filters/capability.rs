use super::super::trait_::{AgentCandidate, FilterPlugin, FormationSpec};

/// Accepts only candidates whose capability set intersects the spec's
/// `required_capabilities` set. If the spec has no required capabilities,
/// every candidate passes (formation can assemble for non-capability work).
pub struct CapabilityFilter;

impl FilterPlugin for CapabilityFilter {
    fn name(&self) -> &'static str {
        "capability"
    }

    fn accept(&self, candidate: &AgentCandidate, spec: &FormationSpec) -> bool {
        if spec.required_capabilities.is_empty() {
            return true;
        }
        spec.required_capabilities
            .iter()
            .any(|req| candidate.capabilities.iter().any(|c| c == req))
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
    fn matching_capability_passes() {
        assert!(CapabilityFilter.accept(&cand(&["github"]), &spec(&["github"])));
    }

    #[test]
    fn non_matching_capability_rejected() {
        assert!(!CapabilityFilter.accept(&cand(&["slack"]), &spec(&["github"])));
    }

    #[test]
    fn empty_spec_accepts_all() {
        assert!(CapabilityFilter.accept(&cand(&[]), &spec(&[])));
    }

    #[test]
    fn any_overlap_passes() {
        assert!(CapabilityFilter.accept(&cand(&["slack", "github"]), &spec(&["github", "nostr"])));
    }
}
