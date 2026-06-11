//! Admission algorithm — run Filters, then Scorers, then pick top-K.
//!
//! Behavior copied from the Kubernetes scheduling framework: any Filter
//! rejecting a candidate excludes it entirely; surviving candidates are
//! ranked by the weighted-sum of Score plugins via the `utility/` module's
//! `WeightedSum` measure.

use std::sync::Arc;

use springtale_cooperation::utility::measure::{Measure, WeightedSum};

use crate::cooperation::FormationId;

use super::error::ComposeError;
use super::trait_::{AgentCandidate, FilterPlugin, FormationSpec, ScorePlugin};
use super::types::{AgentSlot, FormationComposition};

/// Run filters + scorers against `candidates`; produce a FormationComposition
/// or `ComposeError::Empty` when insufficient candidates pass.
pub fn compose_formation(
    candidates: &[AgentCandidate],
    spec: &FormationSpec,
    filters: &[Arc<dyn FilterPlugin>],
    scorers: &[Arc<dyn ScorePlugin>],
) -> Result<FormationComposition, ComposeError> {
    let feasible: Vec<&AgentCandidate> = candidates
        .iter()
        .filter(|c| filters.iter().all(|f| f.accept(c, spec)))
        .collect();

    if feasible.len() < spec.min_members {
        return Err(ComposeError::Empty);
    }

    let measure = WeightedSum;
    let mut scored: Vec<(usize, f32)> = feasible
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let factors: Vec<(f32, f32)> = scorers
                .iter()
                .map(|s| (s.score(c, spec), s.weight()))
                .collect();
            (i, measure.calculate(&factors))
        })
        .collect();

    // Sort by utility descending — top-K candidates become formation members.
    // Per §3.1: composer is Patapon army-select (pick N), not single-best.
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let take = scored.len().min(spec.max_members);
    let members: Vec<AgentSlot> = scored
        .iter()
        .take(take)
        .map(|(idx, _)| {
            let c = feasible[*idx];
            AgentSlot {
                agent_id: c.agent_id,
                capabilities: c.capabilities.clone(),
                role_hint: None,
                ai_config: None,
            }
        })
        .collect();

    Ok(FormationComposition {
        formation_id: FormationId::new(),
        members,
        intent: spec.intent.clone(),
        constraints: spec.constraints.clone(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::filters;
    use super::super::scorers;
    use super::*;
    use crate::cooperation::FormationConstraints;
    use crate::cooperation::cadence::{AgentId, IntentPattern};
    use crate::cooperation::momentum::MomentumTier;
    use crate::cooperation::types::{AgentHealth, AutonomyLevel};

    fn candidate(
        caps: &[&str],
        health: AgentHealth,
        tier: MomentumTier,
        load: f32,
    ) -> AgentCandidate {
        AgentCandidate {
            agent_id: AgentId::new(),
            capabilities: caps
                .iter()
                .map(|s| springtale_cooperation::capability::CapabilityDecl::new(*s))
                .collect(),
            health,
            momentum: tier,
            attention_load: load,
            autonomy_level: AutonomyLevel::ActAutonomously,
        }
    }

    fn spec(req: &[&str], min: usize, max: usize) -> FormationSpec {
        FormationSpec {
            required_capabilities: req
                .iter()
                .map(|s| springtale_cooperation::capability::CapabilityDecl::new(*s))
                .collect(),
            intent: IntentPattern::Execute { plan_id: None },
            constraints: FormationConstraints::default(),
            min_members: min,
            max_members: max,
        }
    }

    fn default_filters() -> Vec<Arc<dyn FilterPlugin>> {
        vec![
            Arc::new(filters::capability::CapabilityFilter),
            Arc::new(filters::health::HealthFilter),
        ]
    }

    fn default_scorers() -> Vec<Arc<dyn ScorePlugin>> {
        vec![
            Arc::new(scorers::load::LoadScorer::new(0.5)),
            Arc::new(scorers::momentum::MomentumScorer::new(0.5)),
        ]
    }

    #[test]
    fn admits_capable_agents_above_minimum() {
        let cands = vec![
            candidate(
                &["github"],
                AgentHealth::Operational,
                MomentumTier::Hot,
                0.2,
            ),
            candidate(
                &["github"],
                AgentHealth::Operational,
                MomentumTier::Warming,
                0.3,
            ),
        ];
        let s = spec(&["github"], 1, 3);
        assert!(compose_formation(&cands, &s, &default_filters(), &default_scorers()).is_ok());
    }

    #[test]
    fn rejects_when_below_minimum_feasible() {
        let cands = vec![candidate(
            &["slack"],
            AgentHealth::Operational,
            MomentumTier::Hot,
            0.0,
        )];
        let s = spec(&["github"], 1, 3);
        let err =
            compose_formation(&cands, &s, &default_filters(), &default_scorers()).unwrap_err();
        assert!(matches!(err, ComposeError::Empty));
    }

    #[test]
    fn caps_members_to_max() {
        let cands: Vec<AgentCandidate> = (0..6)
            .map(|_| {
                candidate(
                    &["github"],
                    AgentHealth::Operational,
                    MomentumTier::Hot,
                    0.1,
                )
            })
            .collect();
        let s = spec(&["github"], 1, 3);
        let comp = compose_formation(&cands, &s, &default_filters(), &default_scorers()).unwrap();
        assert_eq!(comp.members.len(), 3);
    }

    #[test]
    fn ranks_by_scorer_weight() {
        let low_load_high_momentum = candidate(
            &["github"],
            AgentHealth::Operational,
            MomentumTier::Fever,
            0.1,
        );
        let high_load_low_momentum = candidate(
            &["github"],
            AgentHealth::Operational,
            MomentumTier::Cold,
            0.9,
        );
        let target = low_load_high_momentum.agent_id;
        let cands = vec![high_load_low_momentum, low_load_high_momentum];
        let s = spec(&["github"], 1, 1);
        let comp = compose_formation(&cands, &s, &default_filters(), &default_scorers()).unwrap();
        assert_eq!(comp.members.len(), 1);
        assert_eq!(
            comp.members[0].agent_id, target,
            "low-load high-momentum agent should win"
        );
    }
}
