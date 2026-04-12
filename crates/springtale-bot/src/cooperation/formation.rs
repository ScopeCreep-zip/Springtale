//! Formation system — peer agent grouping.
//!
//! Per COOPERATION.pdf §6: "Deep Rock's team has no leader. Nobody
//! assigns the Scout to grapple up. Each class's capabilities intersect
//! with the environment to produce self-organized cooperation.
//! Formations replace the parent→child pipeline."
//!
//! A Formation is a peer group of agents that cooperate through
//! the cadence bus. No member is "parent." Roles emerge from
//! capabilities and context, not assignment.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orchestrator::coordinator::CooperativeBlackboard;
use crate::orchestrator::fuel::FuelBudget;

use super::cadence::{AgentId, IntentPattern};
use super::momentum::{MomentumState, MomentumTier};

/// Unique identifier for a formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FormationId(pub Uuid);

impl Default for FormationId {
    fn default() -> Self {
        Self::new()
    }
}

impl FormationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Health state of an agent in a formation.
///
/// Per COOPERATION.pdf §18.3 (L4D-inspired escalating fragility):
/// Quick-fix recovery leaves the agent degraded. Proper recovery
/// restores full capability. Repeated quick-fixes increase fragility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum AgentHealth {
    /// Full operational capability.
    #[default]
    Operational,
    /// Reduced capability after quick-fix recovery.
    Degraded { recovery_count: u32 },
    /// Incapacitated — needs peer revive (L4D downed state).
    Incapacitated,
    /// Disconnected/dead — can be redeployed (Helldivers reinforce).
    Dead { recoverable: bool },
}

/// Dynamic role of an agent — emerges from context, not assignment.
///
/// Per §23 (Specialization vs Generalization): "The role_hint in
/// the composer (§3.1) should bias, not mandate." Roles are
/// tendencies, not locks. Like Army of Two's weapon-based specialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum DynamicRole {
    /// Default — role not yet determined.
    #[default]
    Unassigned,
    /// Primary task executor.
    Primary { task: String },
    /// Support role (emerged from context, not assigned).
    Support { supporting: AgentId },
    /// Information gatherer (Siege dead→intel pattern).
    Information,
    /// Custom role (connector-specific).
    Custom { name: String },
}

/// A member of a formation.
#[derive(Clone)]
pub struct FormationMember {
    pub agent_id: AgentId,
    pub capabilities: Vec<String>,
    pub current_role: DynamicRole,
    pub awareness: super::awareness::LocalAwareness,
    pub attention_load: f32,
    pub health: AgentHealth,
    /// Per-agent AI adapter. Assigned by the composer at formation spawn time
    /// from config store key `ai:{agent_id}`. Only callable when formation
    /// momentum >= Fever AND agent has "ai_call" capability.
    pub ai_adapter: Option<std::sync::Arc<dyn springtale_ai::AiAdapter>>,
}

impl std::fmt::Debug for FormationMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormationMember")
            .field("agent_id", &self.agent_id)
            .field("capabilities", &self.capabilities)
            .field("current_role", &self.current_role)
            .field("attention_load", &self.attention_load)
            .field("health", &self.health)
            .field("ai_adapter", &self.ai_adapter.is_some())
            .finish()
    }
}

impl FormationMember {
    pub fn new(agent_id: AgentId, capabilities: Vec<String>) -> Self {
        Self {
            agent_id,
            capabilities,
            current_role: DynamicRole::default(),
            awareness: super::awareness::LocalAwareness::default(),
            attention_load: 0.0,
            health: AgentHealth::default(),
            ai_adapter: None,
        }
    }

    /// Create a member with a per-agent AI adapter.
    pub fn with_ai_adapter(
        mut self,
        adapter: std::sync::Arc<dyn springtale_ai::AiAdapter>,
    ) -> Self {
        self.ai_adapter = Some(adapter);
        self.capabilities.push("ai_call".to_owned());
        self
    }

    /// Check if this agent can make AI calls right now.
    /// Requires: has "ai_call" capability AND has an adapter assigned.
    pub fn can_use_ai(&self) -> bool {
        self.ai_adapter.is_some() && self.has_capability("ai_call")
    }

    /// Check if this agent has a specific capability.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }

    /// Check if this agent is operational (can participate in ticks).
    pub fn is_operational(&self) -> bool {
        matches!(
            self.health,
            AgentHealth::Operational | AgentHealth::Degraded { .. }
        )
    }
}

/// Constraints on formation behavior — set by the orchestrator (§3.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationConstraints {
    /// Maximum time the formation can run.
    pub timeout: Duration,
    /// Maximum concurrent actions across all members.
    pub max_concurrent_actions: usize,
    /// Whether the formation is in guard mode (Total War: don't pursue).
    pub guard_mode: bool,
}

impl Default for FormationConstraints {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_concurrent_actions: 8,
            guard_mode: false,
        }
    }
}

/// A formation — peer group of cooperating agents.
///
/// The formation is the fundamental unit of cooperation. It replaces
/// the parent→child pipeline with peer-to-peer coordination through
/// a shared cadence bus, environment (blackboard), and momentum system.
///
/// The optional `orchestrator` is a formation-level AI adapter
/// (same `AiAdapter` trait as per-agent). When present AND momentum
/// is at Fever tier, the orchestrator decomposes the formation's
/// intent into subtasks posted to the blackboard for members to pull
/// (CrewAI/AutoGen manager pattern, gated by Patapon Fever mechanic).
pub struct Formation {
    pub id: FormationId,
    pub members: Vec<FormationMember>,
    pub intent: IntentPattern,
    pub constraints: FormationConstraints,
    pub momentum: MomentumState,
    pub environment: Arc<CooperativeBlackboard>,
    pub fuel: FuelBudget,
    /// Formation-level AI orchestrator. Uses the same `AiAdapter` trait
    /// as per-agent adapters (Ollama, OpenAI, Anthropic).
    /// Loaded from config store key `ai:formation:{id}` at deploy time.
    pub orchestrator: Option<Arc<dyn springtale_ai::AiAdapter>>,
}

impl Formation {
    /// Create a new formation with the given members.
    pub fn new(
        members: Vec<FormationMember>,
        intent: IntentPattern,
        constraints: FormationConstraints,
        fuel: FuelBudget,
    ) -> Self {
        Self {
            id: FormationId::new(),
            members,
            intent,
            constraints,
            momentum: MomentumState::default(),
            environment: Arc::new(CooperativeBlackboard::new()),
            fuel,
            orchestrator: None,
        }
    }

    /// Attach an AI orchestrator to this formation.
    pub fn with_orchestrator(mut self, adapter: Arc<dyn springtale_ai::AiAdapter>) -> Self {
        self.orchestrator = Some(adapter);
        self
    }

    /// Check if AI orchestration is available.
    ///
    /// Requires:
    /// 1. An AI adapter is attached (loaded from config store)
    /// 2. Momentum is at Fever tier (15+ consecutive successes, per Patapon)
    ///
    /// The autonomy ceiling check is done at dispatch time, not here,
    /// because different actions may require different approval levels.
    pub fn can_orchestrate(&self) -> bool {
        self.orchestrator.is_some() && self.momentum.can_use_ai()
    }

    /// Get count of operational members.
    pub fn operational_count(&self) -> usize {
        self.members.iter().filter(|m| m.is_operational()).count()
    }

    /// Get the current momentum tier.
    pub fn momentum_tier(&self) -> MomentumTier {
        self.momentum.tier
    }

    /// Check if the formation is still viable (has operational members).
    pub fn is_viable(&self) -> bool {
        self.operational_count() > 0
    }

    /// Find a member by agent ID.
    pub fn member(&self, agent_id: &AgentId) -> Option<&FormationMember> {
        self.members.iter().find(|m| &m.agent_id == agent_id)
    }

    /// Find a mutable member by agent ID.
    pub fn member_mut(&mut self, agent_id: &AgentId) -> Option<&mut FormationMember> {
        self.members.iter_mut().find(|m| &m.agent_id == agent_id)
    }

    /// Remove permanently dead members from the formation.
    ///
    /// Per L4D pattern: recoverable-dead members stay (can be peer-revived).
    /// Only permanently dead members are removed to free their slots.
    /// Returns the number of members removed.
    pub fn remove_dead_members(&mut self) -> usize {
        let before = self.members.len();
        self.members
            .retain(|m| !matches!(m.health, AgentHealth::Dead { recoverable: false }));
        before - self.members.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_member(cap: &str) -> FormationMember {
        FormationMember::new(AgentId::new(), vec![cap.to_owned()])
    }

    #[test]
    fn test_formation_creation() {
        let formation = Formation::new(
            vec![test_member("slack"), test_member("github")],
            IntentPattern::Reconnoiter {
                target: "issues".to_owned(),
            },
            FormationConstraints::default(),
            FuelBudget::new(10_000),
        );
        assert_eq!(formation.operational_count(), 2);
        assert!(formation.is_viable());
        assert_eq!(formation.momentum_tier(), MomentumTier::Cold);
    }

    #[test]
    fn test_member_capability() {
        let member = test_member("slack");
        assert!(member.has_capability("slack"));
        assert!(!member.has_capability("github"));
    }

    #[test]
    fn test_incapacitated_not_operational() {
        let mut member = test_member("slack");
        member.health = AgentHealth::Incapacitated;
        assert!(!member.is_operational());
    }

    #[test]
    fn test_degraded_still_operational() {
        let mut member = test_member("slack");
        member.health = AgentHealth::Degraded { recovery_count: 1 };
        assert!(member.is_operational());
    }

    #[test]
    fn test_formation_viability() {
        let mut formation = Formation::new(
            vec![test_member("slack")],
            IntentPattern::Stabilize {
                reason: "test".to_owned(),
            },
            FormationConstraints::default(),
            FuelBudget::new(1000),
        );
        assert!(formation.is_viable());

        formation.members[0].health = AgentHealth::Dead { recoverable: false };
        assert!(!formation.is_viable());
    }
}
