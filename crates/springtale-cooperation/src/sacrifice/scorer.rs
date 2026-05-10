//! Sacrifice evaluation — utility AI scoring framework.
//!
//! Per COOPERATION.pdf §24.3 and big-brain architecture (without Bevy):
//! Each of the 4 decision factors produces a 0.0-1.0 score shaped by
//! response curves, composed via ProductOfScorers with compensation
//! (one zero kills the whole score — if ANY factor is critical, don't
//! sacrifice). The result is a continuous utility value, not a binary.
//!
//! This replaces the original boolean 4-gate check with proper utility
//! scoring per the game AI research:
//! - Response curves shape how each factor contributes
//! - ProductOfScorers with compensation handles N-input deflation
//! - Continuous output enables marginal-case nuance

use crate::attention::AttentionEconomy;
use crate::awareness::LocalAwareness;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::momentum::MomentumTier;
use crate::sacrifice::action::SacrificeAction;
use crate::utility::evaluator::{Linear, Power, ResponseCurve, Sigmoid};
use crate::utility::scorer::ProductOfScorers;

/// Lightweight read-only view of formation state for evaluation.
pub struct FormationSnapshot {
    pub member_count: usize,
    pub operational_count: usize,
    pub momentum_tier: MomentumTier,
    pub rally_tokens: u32,
    /// Union of all member capabilities (deduplicated).
    pub capabilities: Vec<CapabilityDecl>,
    /// Capabilities unique to a single member (only one member has it).
    pub unique_capabilities: Vec<(AgentId, CapabilityDecl)>,
}

/// Result of sacrifice evaluation.
pub struct SacrificeEvaluation {
    /// Whether the sacrifice is recommended (utility > threshold).
    pub recommended: bool,
    /// Continuous utility score (0.0-1.0). Higher = more beneficial.
    pub utility_score: f32,
    /// Individual factor scores for transparency/debugging.
    pub net_benefit_score: f32,
    pub recovery_score: f32,
    pub capability_score: f32,
    pub momentum_score: f32,
}

/// Evaluate whether a sacrifice is worth making using utility scoring.
///
/// Per §24.3: Sacrifice must be VOLUNTARY (agent decides based on
/// local awareness), not COMMANDED.
///
/// Four considerations, each producing 0.0-1.0:
/// 1. Net benefit — does the formation gain more than it loses?
/// 2. Recovery path — can the sacrificer recover afterward?
/// 3. Capability preservation — does sacrifice destroy a unique capability?
/// 4. Momentum stability — would losing this agent break momentum?
///
/// Combined via ProductOfScorers with compensation — one zero kills
/// the whole score (can't sacrifice if ANY factor is critical).
pub fn evaluate_sacrifice(
    sacrificer: AgentId,
    beneficiary: AgentId,
    formation: &FormationSnapshot,
    awareness: &LocalAwareness,
    attention: &AttentionEconomy,
) -> SacrificeEvaluation {
    // 1. Net benefit: beneficiary load - sacrificer load, shaped by morale.
    //    Sigmoid curve: creates a dead zone near 0 (don't sacrifice for tiny gains)
    //    and saturates near 1 (big gains don't need to be infinite).
    let raw_benefit = {
        let sacrificer_load = attention.load(&sacrificer);
        let beneficiary_load = attention.load(&beneficiary);
        let morale = awareness.local_morale();
        (beneficiary_load - sacrificer_load) * morale
    };
    // Map raw_benefit: negative = bad (low score), positive = good (high score).
    // Sigmoid centered at 0.5 with steep transition — creates clear distinction
    // between net-negative (score < 0.3) and net-positive (score > 0.7).
    let normalized_benefit = ((raw_benefit + 0.5) / 1.0).clamp(0.0, 1.0);
    let benefit_curve = Sigmoid { midpoint: 0.5, steepness: 10.0 };
    let net_benefit_score = benefit_curve.evaluate(normalized_benefit);

    // 2. Recovery path: rally tokens + member count.
    //    Linear curve: more tokens/members = higher recovery confidence.
    let raw_recovery = {
        let token_factor = formation.rally_tokens as f32 / 3.0;
        let member_factor = if formation.member_count > 2 { 1.0 } else { 0.0 };
        let operational_factor = (formation.operational_count as f32 / 4.0).min(1.0);
        token_factor * 0.4 + member_factor * 0.3 + operational_factor * 0.3
    };
    let recovery_curve = Linear { min: 0.0, max: 1.0 };
    let recovery_score = recovery_curve.evaluate(raw_recovery);

    // 3. Capability preservation: unique capability risk.
    //    Binary-ish via steep sigmoid: if sacrificer has a unique capability,
    //    score drops to near 0. If not, score is near 1.
    let has_unique = formation
        .unique_capabilities
        .iter()
        .any(|(agent, _)| *agent == sacrificer);
    let capability_score = if has_unique { 0.05 } else { 1.0 }; // near-zero if unique

    // 4. Momentum stability: losing agent at high tier with few members.
    //    Power curve (accelerating): small risks score high, big risks score low.
    let raw_momentum_risk = {
        let tier_factor: f32 = match formation.momentum_tier {
            MomentumTier::Cold => 0.0,
            MomentumTier::Warming => 0.3,
            MomentumTier::Hot => 0.7,
            MomentumTier::Fever => 1.0,
        };
        let scarcity_factor: f32 = if formation.operational_count <= 2 { 0.9 } else { 0.3 };
        let distress_factor: f32 = if awareness.distressed_neighbor_count() > 0 { 0.3 } else { 0.0 };
        (tier_factor * scarcity_factor + distress_factor).min(1.0)
    };
    let momentum_curve = Power { min: 0.0, max: 1.0, exponent: 2.0 };
    let momentum_score = 1.0 - momentum_curve.evaluate(raw_momentum_risk); // invert: high risk = low score

    // Compose via ProductOfScorers with compensation.
    // One zero kills the whole — can't sacrifice if ANY factor is critical.
    // Compensation prevents N-input deflation (4 inputs at 0.8 each
    // shouldn't produce 0.41 — compensation lifts it).
    let composite = ProductOfScorers { compensated: true };
    let utility = composite.evaluate(&[
        net_benefit_score,
        recovery_score,
        capability_score,
        momentum_score,
    ]);

    SacrificeEvaluation {
        recommended: utility > 0.5, // threshold: 50% utility = clearly beneficial
        utility_score: utility,
        net_benefit_score,
        recovery_score,
        capability_score,
        momentum_score,
    }
}

/// Per-agent sacrifice consideration — pick the most-loaded peer as
/// beneficiary, evaluate, and return a `SacrificeAction` if the utility
/// clears the recommendation threshold.
///
/// Plan §B9: "agent/step/scan_and_claim.rs final consideration:
/// `sacrifice::scorer::evaluate_action(member, formation_snapshot, awareness)`
/// per `COOPERATION.md §24`. Voluntary, big-brain utility AI. Returns
/// `Option<SacrificeAction>` consumed by the same step."
///
/// Returns `None` when:
/// - the formation has fewer than 2 operational members (no peer to help)
/// - no peer has higher attention load than the sacrificer (no net gain)
/// - the utility evaluator does not recommend the sacrifice
pub fn evaluate_action(
    sacrificer: AgentId,
    formation: &FormationSnapshot,
    awareness: &LocalAwareness,
    attention: &AttentionEconomy,
) -> Option<SacrificeAction> {
    if formation.operational_count < 2 {
        return None;
    }
    let beneficiary = pick_beneficiary(sacrificer, awareness, attention)?;
    let eval = evaluate_sacrifice(sacrificer, beneficiary, formation, awareness, attention);
    if !eval.recommended {
        return None;
    }
    Some(SacrificeAction::Yield {
        sacrificer,
        beneficiary,
        utility: eval.utility_score,
    })
}

/// Pick the peer with the highest attention load as the sacrifice
/// beneficiary. Skips the sacrificer itself and any neighbor whose load
/// is not strictly higher (no net benefit to yield to a less-loaded peer).
fn pick_beneficiary(
    sacrificer: AgentId,
    awareness: &LocalAwareness,
    attention: &AttentionEconomy,
) -> Option<AgentId> {
    let my_load = attention.load(&sacrificer);
    awareness
        .neighbor_states
        .keys()
        .copied()
        .filter(|peer| *peer != sacrificer)
        .map(|peer| (peer, attention.load(&peer)))
        .filter(|(_, load)| *load > my_load)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(peer, _)| peer)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_snapshot(
        operational: usize,
        tier: MomentumTier,
        rally_tokens: u32,
        unique_caps: Vec<(AgentId, CapabilityDecl)>,
    ) -> FormationSnapshot {
        FormationSnapshot {
            member_count: operational,
            operational_count: operational,
            momentum_tier: tier,
            rally_tokens,
            capabilities: vec![CapabilityDecl::new("slack_send"), CapabilityDecl::new("github_read")],
            unique_capabilities: unique_caps,
        }
    }

    #[test]
    fn test_sacrifice_recommended_high_benefit() {
        let sacrificer = AgentId::new();
        let beneficiary = AgentId::new();
        let snapshot = make_snapshot(4, MomentumTier::Warming, 3, vec![]);

        let mut attention = AttentionEconomy::new(&[sacrificer, beneficiary]);
        attention.shift_toward(&beneficiary, 0.3);

        let awareness = LocalAwareness::default();

        let eval = evaluate_sacrifice(sacrificer, beneficiary, &snapshot, &awareness, &attention);
        assert!(eval.recommended, "utility={} should be > 0.4", eval.utility_score);
        assert!(eval.utility_score > 0.4);
        assert!(eval.recovery_score > 0.5);
        assert!(eval.capability_score > 0.9);
    }

    #[test]
    fn test_sacrifice_rejected_unique_capability() {
        let sacrificer = AgentId::new();
        let beneficiary = AgentId::new();
        let snapshot = make_snapshot(
            4,
            MomentumTier::Warming,
            3,
            vec![(sacrificer, CapabilityDecl::new("crypto_sign"))],
        );

        let mut attention = AttentionEconomy::new(&[sacrificer, beneficiary]);
        attention.shift_toward(&beneficiary, 0.3);

        let awareness = LocalAwareness::default();

        let eval = evaluate_sacrifice(sacrificer, beneficiary, &snapshot, &awareness, &attention);
        assert!(!eval.recommended, "unique cap should block: utility={}", eval.utility_score);
        assert!(eval.capability_score < 0.1);
    }

    #[test]
    fn test_sacrifice_rejected_net_negative() {
        let sacrificer = AgentId::new();
        let beneficiary = AgentId::new();
        let snapshot = make_snapshot(4, MomentumTier::Warming, 3, vec![]);

        let mut attention = AttentionEconomy::new(&[sacrificer, beneficiary]);
        attention.shift_toward(&sacrificer, 0.3);

        let awareness = LocalAwareness::default();

        let eval = evaluate_sacrifice(sacrificer, beneficiary, &snapshot, &awareness, &attention);
        // Sacrificer has higher load — net benefit is negative/low.
        // With utility scoring, the overall score should be low enough
        // to not recommend sacrifice.
        assert!(!eval.recommended, "net negative should not recommend: utility={}", eval.utility_score);
    }

    #[test]
    fn test_sacrifice_rejected_momentum_risk() {
        let sacrificer = AgentId::new();
        let beneficiary = AgentId::new();
        let snapshot = make_snapshot(2, MomentumTier::Hot, 3, vec![]);

        let mut attention = AttentionEconomy::new(&[sacrificer, beneficiary]);
        attention.shift_toward(&beneficiary, 0.3);

        let awareness = LocalAwareness::default();

        let eval = evaluate_sacrifice(sacrificer, beneficiary, &snapshot, &awareness, &attention);
        // Hot tier + only 2 members = risky. The momentum_score reflects
        // this via the power curve, but with compensation the overall
        // utility should still be low enough to not recommend.
        assert!(!eval.recommended, "hot+2members should not recommend: utility={}", eval.utility_score);
    }

    #[test]
    fn test_sacrifice_rejected_no_recovery() {
        let sacrificer = AgentId::new();
        let beneficiary = AgentId::new();
        let snapshot = make_snapshot(2, MomentumTier::Cold, 0, vec![]);

        let mut attention = AttentionEconomy::new(&[sacrificer, beneficiary]);
        attention.shift_toward(&beneficiary, 0.3);

        let awareness = LocalAwareness::default();

        let eval = evaluate_sacrifice(sacrificer, beneficiary, &snapshot, &awareness, &attention);
        assert!(eval.recovery_score < 0.4, "0 tokens + 2 members should have low recovery: {}", eval.recovery_score);
    }

    #[test]
    fn test_utility_continuous_not_binary() {
        let sacrificer = AgentId::new();
        let beneficiary = AgentId::new();

        // Marginal case: slight benefit, decent recovery, no unique cap, low tier
        let snapshot = make_snapshot(3, MomentumTier::Warming, 1, vec![]);
        let mut attention = AttentionEconomy::new(&[sacrificer, beneficiary]);
        attention.shift_toward(&beneficiary, 0.1); // slight benefit

        let awareness = LocalAwareness::default();

        let eval = evaluate_sacrifice(sacrificer, beneficiary, &snapshot, &awareness, &attention);
        // Should be in the middle — not clearly yes or no
        assert!(eval.utility_score > 0.1 && eval.utility_score < 0.9,
            "marginal case should be in middle range: {}", eval.utility_score);
    }
}
