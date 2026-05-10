//! Concrete sacrifice action — what the per-agent sacrifice step returns
//! and the bot-side executor applies.
//!
//! Distinct from `SacrificeType` (semantic taxonomy from `COOPERATION.pdf
//! §24.2`). `SacrificeAction` is the small executable form: the agent
//! pipeline can return one and the executor knows how to apply it without
//! cross-crate coupling. New variants land here as the broader sacrifice
//! catalog gets wired (Covering, Expendable, ResourceInvestment).

use crate::cadence::AgentId;

/// What the agent has decided to sacrifice this tick.
///
/// The executor consumes this AFTER the L1 scan returns. If `Some`, the
/// scan's `task_claimed` is dropped and the sacrifice is applied instead.
#[derive(Debug, Clone, PartialEq)]
pub enum SacrificeAction {
    /// The simplest sacrifice: skip this tick entirely so the beneficiary
    /// has more attention/blackboard headroom. No state mutation beyond
    /// emitting a tick report tagged with the sacrificer + beneficiary.
    /// Maps to `SacrificeType::Covering` with no fuel cost.
    Yield {
        sacrificer: AgentId,
        beneficiary: AgentId,
        utility: f32,
    },
}
