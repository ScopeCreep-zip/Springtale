//! Sacrifice consideration step (`COOPERATION.md §24`) — per-agent voluntary
//! self-cost evaluation.
//!
//! Plan §B9: "agent/step/scan_and_claim.rs final consideration. Voluntary,
//! big-brain utility AI. Returns `Option<SacrificeAction>` consumed by the
//! same step." Architecturally separated from `scan` here so the consideration
//! is one named module.
//!
//! Tier-gated to Hot+ — sacrifice requires the formation to be cohesive
//! enough that the peer-help is meaningful (`§24.3` "informed decision needs
//! whole-formation awareness"). Cold/Warming agents skip the evaluation.
//!
//! Builds the per-agent `FormationSnapshot` from the runtime context the
//! step already has access to. `unique_capabilities` is left empty in this
//! pass — the bot-side caller can supply a pre-computed list when it has
//! the full member roster (cooperation crate has no FormationMember type).

use crate::agent::context::AgentContext;
use crate::layer::LayerId;
use crate::momentum::MomentumTier;
use crate::sacrifice::action::SacrificeAction;
use crate::sacrifice::scorer::{evaluate_action, FormationSnapshot};
use crate::{authority, capability::CapabilityDecl};

/// Returns `Some(SacrificeAction)` when the agent's voluntary
/// self-evaluation recommends sacrifice; `None` to fall through.
pub fn run(
    ctx: &AgentContext<'_>,
    rally_tokens: u32,
    member_count: usize,
    unique_capabilities: &[(crate::cadence::AgentId, CapabilityDecl)],
) -> Option<SacrificeAction> {
    if !authority::allows(ctx.momentum.tier, LayerId::L4Contested) {
        // L4 is the "contested" / committed-action layer; sacrifice is a
        // committed action so it gates at the same authority boundary.
        return None;
    }
    if ctx.momentum.tier < MomentumTier::Hot {
        return None;
    }
    let snapshot = FormationSnapshot {
        member_count,
        operational_count: ctx.formation.operational_count,
        momentum_tier: ctx.momentum.tier,
        rally_tokens,
        capabilities: ctx.capabilities.to_vec(),
        unique_capabilities: unique_capabilities.to_vec(),
    };
    evaluate_action(ctx.agent_id, &snapshot, ctx.awareness, ctx.attention)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::attention::AttentionEconomy;
    use crate::awareness::{LocalAwareness, NeighborSnapshot, RoleSignature};
    use crate::cadence::{AgentId, IntentPattern, Tick};
    use crate::context::FormationContext;
    use crate::momentum::MomentumState;
    use crate::supervision::Liveness;
    use crate::types::AgentHealth;
    use std::time::{Duration, Instant};

    fn ctx_at_tier<'a>(
        tick: &'a Tick,
        fc: &'a FormationContext,
        m: &'a MomentumState,
        a: &'a AttentionEconomy,
        aw: &'a LocalAwareness,
        agent_id: AgentId,
        caps: &'a [CapabilityDecl],
    ) -> AgentContext<'a> {
        AgentContext {
            agent_id,
            tick,
            formation: fc,
            momentum: m,
            attention: a,
            capabilities: caps,
            awareness: aw,
        }
    }

    #[test]
    fn cold_tier_skips_evaluation() {
        let me = AgentId::new();
        let tick = Tick {
            sequence: 1,
            timestamp: Instant::now(),
            intent: IntentPattern::Execute { plan_id: None },
            window: Duration::from_millis(33),
        };
        let fc = FormationContext::default();
        let m = MomentumState::default(); // Cold
        let attn = AttentionEconomy::new(&[me]);
        let aw = LocalAwareness::default();
        let caps: Vec<CapabilityDecl> = vec![];
        let result = run(
            &ctx_at_tier(&tick, &fc, &m, &attn, &aw, me, &caps),
            3,
            2,
            &[],
        );
        assert!(result.is_none(), "Cold tier must not evaluate sacrifice");
    }

    #[test]
    fn hot_tier_with_overloaded_peer_yields() {
        // Need 4+ operational members so momentum_score clears the
        // utility threshold at Hot tier (per the existing
        // `test_sacrifice_rejected_momentum_risk` invariant: 2 members
        // at Hot is too risky to sacrifice).
        let me = AgentId::new();
        let peer = AgentId::new();
        let p3 = AgentId::new();
        let p4 = AgentId::new();
        let mut attn = AttentionEconomy::new(&[me, peer, p3, p4]);
        attn.shift_toward(&peer, 0.4);
        let mut aw = LocalAwareness::default();
        aw.update_neighbor(NeighborSnapshot {
            agent_id: peer,
            health: AgentHealth::Operational,
            role: RoleSignature::General,
            fuel_remaining_pct: 0.5,
            last_action_success: true,
            attention_load: 0.7,
            liveness: Liveness::Alive,
            last_updated: Instant::now(),
        });
        let tick = Tick {
            sequence: 1,
            timestamp: Instant::now(),
            intent: IntentPattern::Execute { plan_id: None },
            window: Duration::from_millis(33),
        };
        let fc = FormationContext {
            operational_count: 4,
            member_count: 4,
            ..Default::default()
        };
        let m = MomentumState {
            tier: MomentumTier::Hot,
            ..Default::default()
        };
        let caps: Vec<CapabilityDecl> = vec![];
        let result = run(
            &ctx_at_tier(&tick, &fc, &m, &attn, &aw, me, &caps),
            3,
            4,
            &[],
        );
        assert!(
            matches!(result, Some(SacrificeAction::Yield { sacrificer, beneficiary, .. })
                if sacrificer == me && beneficiary == peer),
            "Hot tier with 4 members + overloaded peer should yield, got {result:?}"
        );
    }
}
