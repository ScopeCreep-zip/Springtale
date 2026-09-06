//! L4 Bidder — default implementation that scores a CFP via `utility/`.
//!
//! Scoring factors, matching Gerkey-Matarić ST-SR-IA bids:
//! - **capability fit**: does the agent actually carry the required capability?
//! - **free capacity**: inverse of attention load (idle agents bid higher).
//! - **momentum readiness**: Hot/Fever agents bid higher than Warming.
//! - **intent alignment**: bonus when the CFP's task aligns with active intent.
//!
//! Combined via `WeightedSum` — additive combination matches the ST-SR-IA
//! utility shape (no single factor is a hard gate at bid time; the gate is
//! the `required_capability` filter before scoring).

use async_trait::async_trait;

use crate::agent::AgentContext;
use crate::capability::CapabilityDecl;
use crate::contract_net::trait_::Bidder;
use crate::contract_net::types::{Bid, CallForProposals};
use crate::momentum::MomentumTier;
use crate::utility::measure::{Measure, WeightedSum};

/// Default bidder: utility scorer. Takes the agent's capability list so the
/// bidder is honest about "I can't do this" instead of bidding on tasks it
/// lacks the capability for.
pub struct UtilityBidder<'a> {
    pub capabilities: &'a [CapabilityDecl],
}

impl<'a> UtilityBidder<'a> {
    pub fn new(capabilities: &'a [CapabilityDecl]) -> Self {
        Self { capabilities }
    }
}

#[async_trait]
impl Bidder for UtilityBidder<'_> {
    async fn evaluate(&self, cfp: &CallForProposals, ctx: &AgentContext<'_>) -> Option<Bid> {
        score(cfp, ctx, self.capabilities).map(|utility| Bid {
            cfp_id: cfp.id,
            bidder: ctx.agent_id,
            utility,
            estimated_completion: cfp.deadline / 2,
            rationale: format!(
                "capability_fit + free_capacity + momentum_readiness = {utility:.3}"
            ),
        })
    }
}

/// Owned-capabilities variant — same scoring, suitable for runner tasks
/// that need an `Arc<dyn Bidder>` outliving any single tick borrow. The
/// borrowed `UtilityBidder<'_>` stays for short-lived callers (per-tick
/// inline scoring); `OwnedUtilityBidder` is what `member_runner.rs`
/// stores in `AgentLoop`.
pub struct OwnedUtilityBidder {
    pub capabilities: Vec<CapabilityDecl>,
}

impl OwnedUtilityBidder {
    pub fn new(capabilities: Vec<CapabilityDecl>) -> Self {
        Self { capabilities }
    }
}

#[async_trait]
impl Bidder for OwnedUtilityBidder {
    async fn evaluate(&self, cfp: &CallForProposals, ctx: &AgentContext<'_>) -> Option<Bid> {
        score(cfp, ctx, &self.capabilities).map(|utility| Bid {
            cfp_id: cfp.id,
            bidder: ctx.agent_id,
            utility,
            estimated_completion: cfp.deadline / 2,
            rationale: format!(
                "owned capability_fit + free_capacity + momentum_readiness = {utility:.3}"
            ),
        })
    }
}

/// Pure scoring function — no I/O, no channels. Easy to unit-test.
///
/// Returns `None` when the agent lacks the required capability (hard gate).
/// Otherwise returns a `[0.0, 1.0]` utility via `WeightedSum`.
/// Formation-scoped bidder: one instance shared by every member of the
/// beat (plan 1.8 / 1.9). It scores each CFP against the capabilities in
/// the bidding member's `AgentContext`, so it never owns one member's
/// capability list the way `UtilityBidder` does.
pub struct ContextBidder;

#[async_trait]
impl Bidder for ContextBidder {
    async fn evaluate(&self, cfp: &CallForProposals, ctx: &AgentContext<'_>) -> Option<Bid> {
        score(cfp, ctx, ctx.capabilities).map(|utility| Bid {
            cfp_id: cfp.id,
            bidder: ctx.agent_id,
            utility,
            estimated_completion: cfp.deadline / 2,
            rationale: format!("beat bid (utility = {utility:.3})"),
        })
    }
}

pub fn score(
    cfp: &CallForProposals,
    ctx: &AgentContext<'_>,
    capabilities: &[CapabilityDecl],
) -> Option<f32> {
    // Hard gate: required capability must be satisfied.
    if let Some(required) = cfp.required_capability.as_ref() {
        if !capabilities.iter().any(|c| c == required) {
            return None;
        }
    } else if !capabilities.iter().any(|c| c == &cfp.task.target_connector) {
        return None;
    }

    let capability_fit = 1.0;
    let free_capacity = (1.0 - ctx.attention.load(&ctx.agent_id)).clamp(0.0, 1.0);
    let momentum_readiness = momentum_bid_weight(ctx.momentum.tier);
    let intent_alignment = intent_overlap(cfp, ctx);

    let score = WeightedSum.calculate(&[
        (capability_fit, 0.30),
        (free_capacity, 0.30),
        (momentum_readiness, 0.25),
        (intent_alignment, 0.15),
    ]);
    Some(score.clamp(0.0, 1.0))
}

fn momentum_bid_weight(tier: MomentumTier) -> f32 {
    match tier {
        MomentumTier::Cold => 0.2,
        MomentumTier::Warming => 0.5,
        MomentumTier::Hot => 0.85,
        MomentumTier::Fever => 1.0,
    }
}

fn intent_overlap(_cfp: &CallForProposals, _ctx: &AgentContext<'_>) -> f32 {
    // Baseline: every agent has mild alignment to any CFP. Refined in step 9
    // when IntentPattern→SubTask overlap is wired through the utility module.
    0.5
}
