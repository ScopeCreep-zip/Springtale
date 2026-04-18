use crate::attention::AttentionEconomy;
use crate::cadence::{AgentId, Tick};
use crate::context::FormationContext;
use crate::momentum::MomentumState;

/// Borrowed view passed into every step/evaluator in the agent pipeline.
///
/// Deliberately narrow — steps see what they need to decide, not the whole
/// Formation. Keeps the agent-side code de-coupled from bot-layer types
/// (FormationMember lives in bot, not here).
#[derive(Debug, Clone, Copy)]
pub struct AgentContext<'a> {
    pub agent_id: AgentId,
    pub tick: &'a Tick,
    pub formation: &'a FormationContext,
    pub momentum: &'a MomentumState,
    pub attention: &'a AttentionEconomy,
}
