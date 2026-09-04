//! Execution context — the cooperation envelope that scopes one
//! chain fire.
//!
//! Every dispatch carries this so the runtime knows which agent in
//! which formation at which momentum tier is firing the rule. Without
//! it, modular recipes silently bypass cooperation invariants:
//!
//! - Two agents in the same formation running the same dedupe rule
//!   would collide on a global key.
//! - Sentinel throttling can't scale with momentum (Fever-tier swarm
//!   throttled like a Cold observer).
//! - Executions log can't answer "what did agent X in formation Y do
//!   on Sunday morning?".
//!
//! Lives in `springtale-cooperation` (not `springtale-core`) because
//! it references [`AgentId`] / [`FormationId`] / [`MomentumTier`] from
//! this crate. `springtale-core::rule::ChainContext` holds within-fire
//! step state; `ExecutionContext` holds the cooperation envelope.
//! Dispatchers thread both.
//!
//! ## ULID vs UUID
//!
//! `ExecutionId` is a [`ulid::Ulid`] — lexicographically sortable by
//! creation time, so `WHERE bot_id = ? ORDER BY id DESC` on the
//! executions table is index-friendly without an extra `started_at`
//! sort key. UUID v4 wastes 122 bits of random for a primary key the
//! database already sorts by insert time.

use serde::{Deserialize, Serialize};
use springtale_core::policy::{ApprovalPolicy, AutonomyLevel};
use springtale_core::rule::RuleId;

use crate::cadence::AgentId;
use crate::momentum::MomentumTier;
use crate::types::FormationId;

/// Identifier for one rule fire. Maps 1:1 to a row in the
/// `executions` table (Phase B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionId(pub ulid::Ulid);

impl ExecutionId {
    /// Mint a fresh ULID at the current wall-clock time.
    pub fn new() -> Self {
        Self(ulid::Ulid::new())
    }
}

impl Default for ExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How the rule was fired. Used to populate the `executions.mode`
/// column and to enable mode-conditional logic in the dispatcher
/// (e.g. `DryRun` mode stubs side-effecting actions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// A cron schedule ticked.
    Cron,
    /// An inbound webhook arrived.
    Webhook,
    /// A connector emitted an event.
    ConnectorEvent,
    /// A watched file changed.
    FileWatch,
    /// User invoked the rule manually (e.g. CLI / IPC "fire now").
    Manual,
    /// Cooperation framework dispatched the rule as part of a
    /// formation tick — bid won via contract-net, blackboard task,
    /// etc.
    Cooperation,
    /// Retry of a previously-failed execution.
    Retry,
    /// Dry-run preview — fetches run for real, side-effecting actions
    /// are stubbed.
    DryRun,
}

/// Cooperation envelope for one rule fire. Threaded through the
/// dispatcher alongside `springtale-core::rule::ChainContext`.
///
/// All fields except `execution_id`, `rule_id`, `mode`, `momentum` are
/// optional. A rule fired from the daemon queue with no formation
/// context still gets an envelope — `agent_id = None`,
/// `formation_id = None`, `momentum = MomentumTier::Warming` (the
/// pre-Phase-0 implicit default, surfaced explicitly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub execution_id: ExecutionId,
    pub rule_id: RuleId,
    /// Agent owning this fire. `None` for global rules (e.g. system
    /// cron, daemon queue entries that aren't agent-scoped).
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    /// Formation the agent belongs to. `None` for solo agents or
    /// global rules.
    #[serde(default)]
    pub formation_id: Option<FormationId>,
    /// Cooperation tier — gates capability via `CapabilityBridge`,
    /// scales sentinel throttling, populates the `momentum` column on
    /// the executions log.
    pub momentum: MomentumTier,
    /// What fired the rule. Surfaces in the executions panel for
    /// "this bot ran at 7am because cron".
    pub mode: ExecutionMode,
    /// Formation `destructive_action_policy`; `AutoApprove` for global rules.
    #[serde(default)]
    pub policy: ApprovalPolicy,
    /// Firing member's autonomy; `ActAutonomously` for global rules.
    #[serde(default)]
    pub autonomy: AutonomyLevel,
}

impl ExecutionContext {
    /// Construct an envelope for a non-cooperative fire (daemon queue,
    /// CLI manual fire, etc.). Defaults `agent_id` / `formation_id`
    /// to `None` and `momentum` to `Warming` (the pre-Phase-0
    /// implicit default, now surfaced).
    pub fn for_global(rule_id: RuleId, mode: ExecutionMode) -> Self {
        Self {
            execution_id: ExecutionId::new(),
            rule_id,
            agent_id: None,
            formation_id: None,
            momentum: MomentumTier::Warming,
            mode,
            policy: ApprovalPolicy::default(),
            autonomy: AutonomyLevel::default(),
        }
    }

    /// Construct an envelope for a formation-tick fire. Caller
    /// supplies the firing agent, the formation, the formation's
    /// current momentum tier, its `destructive_action_policy`, and
    /// the firing member's autonomy level.
    pub fn for_formation(
        rule_id: RuleId,
        agent_id: AgentId,
        formation_id: FormationId,
        momentum: MomentumTier,
        mode: ExecutionMode,
        policy: ApprovalPolicy,
        autonomy: AutonomyLevel,
    ) -> Self {
        Self {
            execution_id: ExecutionId::new(),
            rule_id,
            agent_id: Some(agent_id),
            formation_id: Some(formation_id),
            momentum,
            mode,
            policy,
            autonomy,
        }
    }

    /// Construct an envelope for an agent-scoped fire (single-agent
    /// rule, no formation). Common for solo bots — Telegram echo,
    /// Signal auto-reply, etc.
    pub fn for_agent(
        rule_id: RuleId,
        agent_id: AgentId,
        momentum: MomentumTier,
        mode: ExecutionMode,
    ) -> Self {
        Self {
            execution_id: ExecutionId::new(),
            rule_id,
            agent_id: Some(agent_id),
            formation_id: None,
            momentum,
            mode,
            policy: ApprovalPolicy::default(),
            autonomy: AutonomyLevel::default(),
        }
    }

    /// `true` when this fire is a dry-run preview — the dispatcher
    /// should stub side-effecting actions.
    pub fn is_dry_run(&self) -> bool {
        matches!(self.mode, ExecutionMode::DryRun)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn execution_id_is_unique_per_call() {
        let a = ExecutionId::new();
        let b = ExecutionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_round_trips_through_serde() {
        let id = ExecutionId::new();
        let s = serde_json::to_string(&id).unwrap();
        let back: ExecutionId = serde_json::from_str(&s).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn for_global_omits_agent_and_formation() {
        let ctx = ExecutionContext::for_global(RuleId(Uuid::new_v4()), ExecutionMode::Cron);
        assert!(ctx.agent_id.is_none());
        assert!(ctx.formation_id.is_none());
        assert_eq!(ctx.momentum, MomentumTier::Warming);
        assert_eq!(ctx.mode, ExecutionMode::Cron);
        assert_eq!(ctx.policy, ApprovalPolicy::AutoApprove);
        assert_eq!(ctx.autonomy, AutonomyLevel::ActAutonomously);
    }

    #[test]
    fn for_agent_carries_agent_but_no_formation() {
        let ctx = ExecutionContext::for_agent(
            RuleId(Uuid::new_v4()),
            AgentId(Uuid::new_v4()),
            MomentumTier::Hot,
            ExecutionMode::ConnectorEvent,
        );
        assert!(ctx.agent_id.is_some());
        assert!(ctx.formation_id.is_none());
        assert_eq!(ctx.momentum, MomentumTier::Hot);
    }

    #[test]
    fn for_formation_carries_both() {
        let ctx = ExecutionContext::for_formation(
            RuleId(Uuid::new_v4()),
            AgentId(Uuid::new_v4()),
            FormationId(Uuid::new_v4()),
            MomentumTier::Fever,
            ExecutionMode::Cooperation,
            ApprovalPolicy::AlwaysRequire,
            AutonomyLevel::ActWithApproval,
        );
        assert!(ctx.agent_id.is_some());
        assert!(ctx.formation_id.is_some());
        assert_eq!(ctx.momentum, MomentumTier::Fever);
        assert_eq!(ctx.policy, ApprovalPolicy::AlwaysRequire);
        assert_eq!(ctx.autonomy, AutonomyLevel::ActWithApproval);
    }

    #[test]
    fn is_dry_run_only_true_for_dry_run_mode() {
        let dry = ExecutionContext {
            execution_id: ExecutionId::new(),
            rule_id: RuleId(Uuid::new_v4()),
            agent_id: None,
            formation_id: None,
            momentum: MomentumTier::Warming,
            mode: ExecutionMode::DryRun,
            policy: ApprovalPolicy::default(),
            autonomy: AutonomyLevel::default(),
        };
        assert!(dry.is_dry_run());
        let live = ExecutionContext::for_global(RuleId(Uuid::new_v4()), ExecutionMode::Cron);
        assert!(!live.is_dry_run());
    }

    #[test]
    fn round_trips_through_serde_json() {
        let ctx = ExecutionContext::for_agent(
            RuleId(Uuid::new_v4()),
            AgentId(Uuid::new_v4()),
            MomentumTier::Warming,
            ExecutionMode::Webhook,
        );
        let s = serde_json::to_string(&ctx).unwrap();
        let back: ExecutionContext = serde_json::from_str(&s).unwrap();
        assert_eq!(ctx.execution_id, back.execution_id);
        assert_eq!(ctx.rule_id.0, back.rule_id.0);
        assert_eq!(ctx.mode, back.mode);
    }

    #[test]
    fn missing_policy_and_autonomy_deserialize_to_defaults() {
        let json = serde_json::json!({
            "execution_id": ExecutionId::new(),
            "rule_id": RuleId(Uuid::new_v4()),
            "momentum": MomentumTier::Warming,
            "mode": "cron",
        });
        let ctx: ExecutionContext = serde_json::from_value(json).unwrap();
        assert_eq!(ctx.policy, ApprovalPolicy::AutoApprove);
        assert_eq!(ctx.autonomy, AutonomyLevel::ActAutonomously);
    }
}
