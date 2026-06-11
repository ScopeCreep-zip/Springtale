//! Consideration — reads agent/formation state and produces a score.
//!
//! Per IAUS architecture: a Consideration is the "eyes" of the AI.
//! It looks at one aspect of the world and produces a normalized
//! 0.0-1.0 value. Multiple considerations feed into composite
//! scorers to produce a final utility value.
//!
//! Considerations are stateless functions, not persistent objects.
//! They read current state and return a score — no side effects.

/// A function that reads state and produces a 0.0-1.0 score.
///
/// Considerations are the inputs to the utility scoring pipeline.
/// Each one measures one aspect of the agent/formation state:
/// - "How loaded am I?" (attention_load → 0.0-1.0)
/// - "How healthy is my neighbor?" (awareness → 0.0-1.0)
/// - "How urgent is this task?" (priority → 0.0-1.0)
pub trait Consideration: std::fmt::Debug + Send + Sync {
    /// Produce a score for the given context. Must be in 0.0-1.0 range.
    fn score(&self, ctx: &ConsiderationContext<'_>) -> f32;

    /// Human-readable name for debugging/tracing.
    fn name(&self) -> &'static str;
}

/// Context passed to considerations — everything an agent can perceive.
///
/// Per RTS fog-of-war principle: this contains only what the agent
/// CAN see (local awareness, own state), not global omniscient state.
pub struct ConsiderationContext<'a> {
    /// This agent's attention load (0.0-1.0).
    pub attention_load: f32,
    /// This agent's health status.
    pub health: &'a crate::types::AgentHealth,
    /// This agent's consecutive failure count.
    pub consecutive_failures: usize,
    /// This agent's capabilities.
    pub capabilities: &'a [crate::capability::CapabilityDecl],
    /// This agent's local awareness of neighbors.
    pub awareness: &'a crate::awareness::LocalAwareness,
    /// Formation momentum tier.
    pub momentum_tier: crate::momentum::MomentumTier,
    /// Formation operational count.
    pub operational_count: usize,
    /// Formation rally tokens remaining.
    pub rally_tokens: u32,
}

/// Consideration: how much free capacity does this agent have?
/// High attention load → low score (agent is busy).
#[derive(Debug)]
pub struct FreeCapacity;

impl Consideration for FreeCapacity {
    fn score(&self, ctx: &ConsiderationContext<'_>) -> f32 {
        1.0 - ctx.attention_load
    }
    fn name(&self) -> &'static str {
        "FreeCapacity"
    }
}

/// Consideration: how healthy are my neighbors? (Total War morale signal)
/// All healthy → 1.0, many distressed → low score.
#[derive(Debug)]
pub struct NeighborMorale;

impl Consideration for NeighborMorale {
    fn score(&self, ctx: &ConsiderationContext<'_>) -> f32 {
        ctx.awareness.local_morale()
    }
    fn name(&self) -> &'static str {
        "NeighborMorale"
    }
}

/// Consideration: how stable is the formation?
/// High momentum + many operational members → high score.
#[derive(Debug)]
pub struct FormationStability;

impl Consideration for FormationStability {
    fn score(&self, ctx: &ConsiderationContext<'_>) -> f32 {
        let momentum_factor = match ctx.momentum_tier {
            crate::momentum::MomentumTier::Cold => 0.1,
            crate::momentum::MomentumTier::Warming => 0.4,
            crate::momentum::MomentumTier::Hot => 0.7,
            crate::momentum::MomentumTier::Fever => 1.0,
        };
        let member_factor = (ctx.operational_count as f32 / 4.0).min(1.0);
        (momentum_factor * 0.6 + member_factor * 0.4).clamp(0.0, 1.0)
    }
    fn name(&self) -> &'static str {
        "FormationStability"
    }
}

/// Consideration: is this agent in danger of failing?
/// High consecutive failures → high danger score.
#[derive(Debug)]
pub struct FailureDanger;

impl Consideration for FailureDanger {
    fn score(&self, ctx: &ConsiderationContext<'_>) -> f32 {
        // 5 failures = transformation trigger threshold
        (ctx.consecutive_failures as f32 / 5.0).min(1.0)
    }
    fn name(&self) -> &'static str {
        "FailureDanger"
    }
}

/// Consideration: can the formation afford a loss?
/// Rally tokens remaining relative to max (3).
#[derive(Debug)]
pub struct RecoveryBudget;

impl Consideration for RecoveryBudget {
    fn score(&self, ctx: &ConsiderationContext<'_>) -> f32 {
        ctx.rally_tokens as f32 / 3.0
    }
    fn name(&self) -> &'static str {
        "RecoveryBudget"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::awareness::LocalAwareness;
    use crate::types::AgentHealth;

    fn make_ctx(
        load: f32,
        failures: usize,
        momentum: crate::momentum::MomentumTier,
    ) -> ConsiderationContext<'static> {
        // Leak awareness to get 'static — only in tests
        let awareness = Box::leak(Box::new(LocalAwareness::default()));
        let health = Box::leak(Box::new(AgentHealth::Operational));
        let caps: &'static [crate::capability::CapabilityDecl] = Box::leak(Box::new(Vec::new()));
        ConsiderationContext {
            attention_load: load,
            health,
            consecutive_failures: failures,
            capabilities: caps,
            awareness,
            momentum_tier: momentum,
            operational_count: 3,
            rally_tokens: 2,
        }
    }

    #[test]
    fn test_free_capacity() {
        let ctx = make_ctx(0.3, 0, crate::momentum::MomentumTier::Hot);
        let score = FreeCapacity.score(&ctx);
        assert!((score - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_failure_danger() {
        let ctx = make_ctx(0.0, 3, crate::momentum::MomentumTier::Cold);
        let score = FailureDanger.score(&ctx);
        assert!((score - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn test_formation_stability() {
        let ctx = make_ctx(0.0, 0, crate::momentum::MomentumTier::Fever);
        let score = FormationStability.score(&ctx);
        assert!(score > 0.8);
    }

    #[test]
    fn test_recovery_budget() {
        let ctx = make_ctx(0.0, 0, crate::momentum::MomentumTier::Cold);
        let score = RecoveryBudget.score(&ctx);
        assert!((score - 2.0 / 3.0).abs() < 0.01);
    }
}
