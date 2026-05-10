use crate::attention::AttentionEconomy;
use crate::awareness::LocalAwareness;
use crate::cadence::{AgentId, Tick};
use crate::capability::CapabilityDecl;
use crate::context::FormationContext;
use crate::momentum::MomentumState;

/// Borrowed view passed into every step/evaluator in the agent pipeline.
///
/// Deliberately narrow — steps see what they need to decide, not the whole
/// Formation. Keeps the agent-side code de-coupled from bot-layer types
/// (FormationMember lives in bot, not here). Mutable awareness is passed
/// separately to the steps that update it (`react`); the read-only
/// `awareness` field here lets `scan` consult peer TickReports for the
/// B5 priority-merge at Warming+.
#[derive(Debug, Clone, Copy)]
pub struct AgentContext<'a> {
    pub agent_id: AgentId,
    pub tick: &'a Tick,
    pub formation: &'a FormationContext,
    pub momentum: &'a MomentumState,
    pub attention: &'a AttentionEconomy,
    pub capabilities: &'a [CapabilityDecl],
    /// Read-only snapshot of the agent's local awareness — peer health,
    /// roles, and last-tick reports. Consumed by `agent::step::scan` at
    /// Warming+ tier to weight priority by neighbor recent successes
    /// (B5 in plan §B5: "neighbor TickReports merged into priority").
    pub awareness: &'a LocalAwareness,
}
