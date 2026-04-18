use crate::cooperation::FormationConstraints;
use crate::cooperation::cadence::{AgentId, IntentPattern};
use crate::cooperation::capability::CapabilityDecl;
use crate::cooperation::momentum::MomentumTier;
use crate::cooperation::types::{AgentHealth, AutonomyLevel};

/// Candidate agent offered to the composer. Fields are the minimum the
/// default filter and scorer plugins need; admission policies that want
/// more signal (uptime, reputation) extend by subtyping or adding new
/// plugins that draw from other sources.
#[derive(Debug, Clone)]
pub struct AgentCandidate {
    pub agent_id: AgentId,
    pub capabilities: Vec<CapabilityDecl>,
    pub health: AgentHealth,
    pub momentum: MomentumTier,
    pub attention_load: f32,
    pub autonomy_level: AutonomyLevel,
}

/// Formation requirements — what caller wants built.
#[derive(Debug, Clone)]
pub struct FormationSpec {
    pub required_capabilities: Vec<CapabilityDecl>,
    pub intent: IntentPattern,
    pub constraints: FormationConstraints,
    pub min_members: usize,
    pub max_members: usize,
}

/// Hard filter — K8s predicate. Returning `false` rejects the candidate.
pub trait FilterPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn accept(&self, candidate: &AgentCandidate, spec: &FormationSpec) -> bool;
}

/// Soft scorer — K8s priority. Returns `[0.0, 1.0]` with a weight the
/// admission function folds into a weighted sum.
pub trait ScorePlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn score(&self, candidate: &AgentCandidate, spec: &FormationSpec) -> f32;
    fn weight(&self) -> f32;
}
