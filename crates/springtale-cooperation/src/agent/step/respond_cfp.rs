//! L4 respond-to-CFP step — when an active CFP is in scope, evaluate via
//! a `Bidder` and return a bid if willing to commit (`COOPERATION.md §11`
//! As Dusk Falls voting / Contract Net pattern).
//!
//! Trait-bounded per plan §A2 — `&dyn Bidder` so the bidder impl
//! (`UtilityBidder` in production, mocks in tests) is swappable.

use crate::agent::context::AgentContext;
use crate::authority;
use crate::contract_net::trait_::Bidder;
use crate::contract_net::types::{Bid, CallForProposals};
use crate::layer::LayerId;

pub async fn run(
    bidder: &dyn Bidder,
    cfp: &CallForProposals,
    ctx: &AgentContext<'_>,
) -> Option<Bid> {
    if !authority::allows(ctx.momentum.tier, LayerId::L4Contested) {
        return None;
    }
    bidder.evaluate(cfp, ctx).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::action::SubTask;
    use crate::attention::AttentionEconomy;
    use crate::cadence::{AgentId, IntentPattern, Tick};
    use crate::capability::CapabilityDecl;
    use crate::context::FormationContext;
    use crate::contract_net::bid::evaluate::UtilityBidder;
    use crate::contract_net::cfp::descriptor;
    use crate::momentum::{MomentumState, MomentumTier};
    use std::time::Duration;

    #[tokio::test]
    async fn capable_agent_at_hot_produces_bid() {
        let agent = AgentId::new();
        let tick = Tick {
            sequence: 1,
            timestamp: std::time::Instant::now(),
            intent: IntentPattern::Execute { plan_id: None },
            window: Duration::from_millis(33),
        };
        let fc = FormationContext {
            momentum_tier: MomentumTier::Hot,
            ..Default::default()
        };
        let momentum = MomentumState {
            tier: MomentumTier::Hot,
            ..Default::default()
        };
        let attention = AttentionEconomy::new(&[agent]);
        let caps: Vec<CapabilityDecl> = vec!["github".into()];
        let aw = crate::awareness::LocalAwareness::default();
        let ctx = AgentContext {
            agent_id: agent,
            tick: &tick,
            formation: &fc,
            momentum: &momentum,
            attention: &attention,
            capabilities: &caps,
            awareness: &aw,
        };
        let cfp = descriptor::for_task(
            AgentId::new(),
            SubTask {
                id: uuid::Uuid::new_v4(),
                target_connector: "github".into(),
                action_name: "act".into(),
                params: serde_json::json!({}),
                priority: 1,
                assigned_to: None,
                description: String::new(),
            },
            Duration::from_millis(100),
            Some("github".into()),
        );
        let bidder = UtilityBidder::new(&caps);
        let bid = run(&bidder, &cfp, &ctx).await;
        assert!(bid.is_some());
        assert!(bid.unwrap().utility > 0.0);
    }

    #[tokio::test]
    async fn cold_tier_blocks_bid() {
        let agent = AgentId::new();
        let tick = Tick {
            sequence: 1,
            timestamp: std::time::Instant::now(),
            intent: IntentPattern::Execute { plan_id: None },
            window: Duration::from_millis(33),
        };
        let fc = FormationContext::default();
        let momentum = MomentumState::default();
        let attention = AttentionEconomy::new(&[agent]);
        let caps: Vec<CapabilityDecl> = vec!["github".into()];
        let aw = crate::awareness::LocalAwareness::default();
        let ctx = AgentContext {
            agent_id: agent,
            tick: &tick,
            formation: &fc,
            momentum: &momentum,
            attention: &attention,
            capabilities: &caps,
            awareness: &aw,
        };
        let cfp = descriptor::for_task(
            AgentId::new(),
            SubTask {
                id: uuid::Uuid::new_v4(),
                target_connector: "github".into(),
                action_name: "act".into(),
                params: serde_json::json!({}),
                priority: 1,
                assigned_to: None,
                description: String::new(),
            },
            Duration::from_millis(100),
            Some("github".into()),
        );
        let bidder = UtilityBidder::new(&caps);
        assert!(run(&bidder, &cfp, &ctx).await.is_none());
    }
}
