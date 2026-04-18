//! L4 respond-to-CFP step — if an active CFP matches our capabilities,
//! evaluate and produce a bid.

use crate::agent::AgentContext;
use crate::authority;
use crate::capability::CapabilityDecl;
use crate::contract_net::bid::evaluate;
use crate::contract_net::types::{Bid, CallForProposals};
use crate::layer::LayerId;

/// Evaluate an incoming CFP and return a bid if the agent is willing to
/// commit. Returns `None` when the agent lacks capability, is at too low
/// a tier, or the utility score is zero.
pub fn step_respond_cfp(
    cfp: &CallForProposals,
    capabilities: &[CapabilityDecl],
    ctx: &AgentContext<'_>,
) -> Option<Bid> {
    if !authority::allows(ctx.momentum.tier, LayerId::L4Contested) {
        return None;
    }

    evaluate::score(cfp, ctx, capabilities).map(|utility| Bid {
        cfp_id: cfp.id,
        bidder: ctx.agent_id,
        utility,
        estimated_completion: cfp.deadline / 2,
        rationale: format!("step_respond_cfp utility={utility:.3}"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::action::SubTask;
    use crate::attention::AttentionEconomy;
    use crate::cadence::{AgentId, IntentPattern, Tick};
    use crate::context::FormationContext;
    use crate::contract_net::cfp::descriptor;
    use crate::momentum::{MomentumState, MomentumTier};
    use crate::types::FormationConstraints;

    fn make_ctx<'a>(
        agent: AgentId,
        tick: &'a Tick,
        formation: &'a FormationContext,
        momentum: &'a MomentumState,
        attention: &'a AttentionEconomy,
    ) -> AgentContext<'a> {
        AgentContext {
            agent_id: agent,
            tick,
            formation,
            momentum,
            attention,
        }
    }

    #[test]
    fn capable_agent_at_hot_produces_bid() {
        let agent = AgentId::new();
        let tick = Tick { sequence: 1, timestamp: std::time::Instant::now(), intent: IntentPattern::Execute { plan_id: None }, window: Duration::from_millis(33) };
        let formation = FormationContext { intent: IntentPattern::Execute { plan_id: None }, momentum_tier: MomentumTier::Hot, constraints: FormationConstraints::default(), guard_mode: false, operational_count: 2, member_count: 2, paused: false };
        let momentum = MomentumState { tier: MomentumTier::Hot, ..Default::default() };
        let attention = AttentionEconomy::new(&[agent]);
        let ctx = make_ctx(agent, &tick, &formation, &momentum, &attention);
        let cfp = descriptor::for_task(AgentId::new(), SubTask { id: uuid::Uuid::new_v4(), target_connector: "github".into(), action_name: "act".into(), params: serde_json::json!({}), priority: 1, assigned_to: None, description: String::new() }, Duration::from_millis(100), Some("github".into()));
        let bid = step_respond_cfp(&cfp, &["github".into()], &ctx);
        assert!(bid.is_some());
        assert!(bid.unwrap().utility > 0.0);
    }

    #[test]
    fn cold_tier_blocks_bid() {
        let agent = AgentId::new();
        let tick = Tick { sequence: 1, timestamp: std::time::Instant::now(), intent: IntentPattern::Execute { plan_id: None }, window: Duration::from_millis(33) };
        let formation = FormationContext { intent: IntentPattern::Execute { plan_id: None }, momentum_tier: MomentumTier::Cold, constraints: FormationConstraints::default(), guard_mode: false, operational_count: 2, member_count: 2, paused: false };
        let momentum = MomentumState::default();
        let attention = AttentionEconomy::new(&[agent]);
        let ctx = make_ctx(agent, &tick, &formation, &momentum, &attention);
        let cfp = descriptor::for_task(AgentId::new(), SubTask { id: uuid::Uuid::new_v4(), target_connector: "github".into(), action_name: "act".into(), params: serde_json::json!({}), priority: 1, assigned_to: None, description: String::new() }, Duration::from_millis(100), Some("github".into()));
        assert!(step_respond_cfp(&cfp, &["github".into()], &ctx).is_none());
    }

    #[test]
    fn non_capable_agent_returns_none() {
        let agent = AgentId::new();
        let tick = Tick { sequence: 1, timestamp: std::time::Instant::now(), intent: IntentPattern::Execute { plan_id: None }, window: Duration::from_millis(33) };
        let formation = FormationContext { intent: IntentPattern::Execute { plan_id: None }, momentum_tier: MomentumTier::Hot, constraints: FormationConstraints::default(), guard_mode: false, operational_count: 2, member_count: 2, paused: false };
        let momentum = MomentumState { tier: MomentumTier::Hot, ..Default::default() };
        let attention = AttentionEconomy::new(&[agent]);
        let ctx = make_ctx(agent, &tick, &formation, &momentum, &attention);
        let cfp = descriptor::for_task(AgentId::new(), SubTask { id: uuid::Uuid::new_v4(), target_connector: "github".into(), action_name: "act".into(), params: serde_json::json!({}), priority: 1, assigned_to: None, description: String::new() }, Duration::from_millis(100), Some("github".into()));
        assert!(step_respond_cfp(&cfp, &["slack".into()], &ctx).is_none());
    }
}
