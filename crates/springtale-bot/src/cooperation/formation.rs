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

use crate::cooperation::blackboard::CooperativeBlackboard;
use crate::orchestrator::fuel::FuelBudget;

use tokio::sync::{broadcast, watch};

use springtale_cooperation::attention::AttentionBroker;
use springtale_cooperation::awareness::LocalAwareness;
use springtale_cooperation::cadence::{AgentId, IntentPattern};
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::comms::bus::FormationBusSubscription;
use springtale_cooperation::comms::FormationBus;
use springtale_cooperation::commit::CommitBarrier;
use springtale_cooperation::consensus::ConsensusEngine;
use springtale_cooperation::context::FormationContext;
use springtale_cooperation::mental_model::SharedMentalModel;
use springtale_cooperation::momentum::{MomentumState, MomentumTier};
use springtale_cooperation::pacing::PacingManager;
use springtale_cooperation::peer::PeerMsg;
use springtale_cooperation::rally::RallyState;
use springtale_cooperation::role::{DynamicRoleTrait, GeneralAgent};
use springtale_cooperation::supervision::{FormationSupervisor, Liveness};
use springtale_cooperation::types::{AgentHealth, FormationConstraints, FormationId};

/// A member of a formation.
#[derive(Clone)]
pub struct FormationMember {
    pub agent_id: AgentId,
    pub capabilities: Vec<CapabilityDecl>,
    pub role: Box<dyn DynamicRoleTrait>,
    pub awareness: LocalAwareness,
    pub health: AgentHealth,
    pub liveness: Liveness,
    pub fuel_remaining: FuelBudget,
    pub last_report_tick: u64,
    /// Consecutive tick failures for this member. Used by transformation
    /// trigger (§14) — 5+ failures → ToSupportAgent.
    pub consecutive_failures: usize,
    /// The agent's current task with lifecycle tracking.
    /// Per Spring engine: command queue front = current task.
    /// None when agent is idle (monitoring connector).
    pub active_task: Option<springtale_cooperation::action_state::ActiveTask>,
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
            .field("role", &self.role.name())
            .field("health", &self.health)
            .field("liveness", &self.liveness)
            .field("ai_adapter", &self.ai_adapter.is_some())
            .finish()
    }
}

impl FormationMember {
    pub fn new(agent_id: AgentId, capabilities: Vec<CapabilityDecl>) -> Self {
        let role: Box<dyn DynamicRoleTrait> = Box::new(GeneralAgent::new(capabilities.clone()));
        Self {
            agent_id,
            capabilities,
            role,
            awareness: LocalAwareness::default(),
            health: AgentHealth::default(),
            liveness: Liveness::Alive,
            fuel_remaining: FuelBudget::new(1000),
            last_report_tick: 0,
            consecutive_failures: 0,
            active_task: None,
            ai_adapter: None,
        }
    }

    /// Create with string capabilities (convenience for backward compat).
    pub fn from_strings(agent_id: AgentId, capabilities: Vec<String>) -> Self {
        let caps: Vec<CapabilityDecl> = capabilities.into_iter().map(CapabilityDecl::from).collect();
        Self::new(agent_id, caps)
    }

    /// Create a member with a per-agent AI adapter.
    pub fn with_ai_adapter(
        mut self,
        adapter: std::sync::Arc<dyn springtale_ai::AiAdapter>,
    ) -> Self {
        self.ai_adapter = Some(adapter);
        self.capabilities.push("ai_call".into());
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

    /// Tier-aware capability view — projects `capabilities` through the
    /// momentum `Binder` so callers see both base connector capabilities and
    /// the cooperation primitives unlocked at `tier`.
    pub fn effective_capabilities(
        &self,
        tier: springtale_cooperation::MomentumTier,
    ) -> Vec<springtale_cooperation::capability::CapabilityDecl> {
        use springtale_cooperation::capability::Binder;
        springtale_cooperation::capability::DefaultBinder.effective(&self.capabilities, tier)
    }

    /// Check if this agent is operational (can participate in ticks).
    pub fn is_operational(&self) -> bool {
        matches!(
            self.health,
            AgentHealth::Operational | AgentHealth::Degraded { .. }
        )
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
    /// When `true`, tick processing skips this formation entirely.
    pub paused: bool,
    pub constraints: FormationConstraints,
    pub momentum: MomentumState,
    pub environment: Arc<CooperativeBlackboard>,
    pub fuel: FuelBudget,
    /// Formation-level AI orchestrator. Uses the same `AiAdapter` trait
    /// as per-agent adapters (Ollama, OpenAI, Anthropic).
    /// Loaded from config store key `ai:formation:{id}` at deploy time.
    pub orchestrator: Option<Arc<dyn springtale_ai::AiAdapter>>,

    // ── Cooperation subsystem state ──────────────────────────────────
    /// L4D Director-inspired work/rest cycle management (§22).
    pub pacing: PacingManager,
    /// Rally tokens for formation self-healing (§15, Monster Hunter carts).
    pub rally: RallyState,
    /// Zero-sum workload distribution (§9, Army of Two aggro meter).
    /// ArcSwap-backed for concurrent lock-free reads.
    pub attention_broker: AttentionBroker,
    /// Erlang OTP-style supervision with liveness probes (§M2).
    pub supervisor: FormationSupervisor,
    /// Accumulated cooperative knowledge (§21).
    pub mental_model: SharedMentalModel,
    /// Active consensus votes (§11, As Dusk Falls voting).
    pub consensus: ConsensusEngine,
    /// Active synchronized commit barriers (§12, Splinter Cell dual breach).
    pub active_commits: Vec<CommitBarrier>,

    // ── Communication (§6, §19) ─────────────────────────────────
    /// Multi-layer communication bus for inter-agent messaging (§19).
    pub bus: FormationBus,
    /// Initial bus subscription (for the formation-level tick processor).
    pub bus_sub: FormationBusSubscription,
    /// Peer event broadcast — structural events (join/leave/down).
    pub peer_tx: broadcast::Sender<PeerMsg>,
    /// Shared context watch — intent, momentum, constraints broadcast to all members.
    pub context_tx: watch::Sender<FormationContext>,
}

impl Formation {
    /// Create a new formation with the given members.
    ///
    /// Per Total War supply model: fuel is a formation-level constraint set
    /// at composition time. `FuelBudget` is created from
    /// `constraints.fuel_budget` so there's one source of truth for the
    /// initial allocation.
    pub fn new(
        members: Vec<FormationMember>,
        intent: IntentPattern,
        constraints: FormationConstraints,
    ) -> Self {
        let fuel = FuelBudget::new(constraints.fuel_budget.0);
        let agent_ids: Vec<AgentId> = members.iter().map(|m| m.agent_id).collect();
        let (bus, bus_sub) = FormationBus::new();
        let (peer_tx, _) = broadcast::channel::<PeerMsg>(64);
        let initial_context = FormationContext {
            intent: intent.clone(),
            momentum_tier: MomentumTier::Cold,
            constraints: constraints.clone(),
            guard_mode: constraints.guard_mode,
            operational_count: members.iter().filter(|m| m.is_operational()).count(),
            member_count: members.len(),
            paused: false,
        };
        let (context_tx, _) = watch::channel(initial_context);

        Self {
            id: FormationId::new(),
            intent,
            paused: false,
            constraints,
            momentum: MomentumState::default(),
            environment: Arc::new(CooperativeBlackboard::new()),
            fuel,
            orchestrator: None,
            pacing: PacingManager::default(),
            rally: RallyState::default(),
            attention_broker: AttentionBroker::for_agents(&agent_ids),
            supervisor: FormationSupervisor::default(),
            mental_model: SharedMentalModel::default(),
            consensus: ConsensusEngine::new(),
            active_commits: Vec::new(),
            bus,
            bus_sub,
            peer_tx,
            context_tx,
            members,
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
        self.orchestrator.is_some() && self.momentum.can_ai_orchestrate()
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

    /// Broadcast updated context to all watching members.
    ///
    /// Called after any state change (momentum update, intent change, member
    /// join/leave) so members see the latest formation state via their
    /// watch::Receiver<FormationContext>.
    pub fn broadcast_context(&self) {
        self.context_tx.send_modify(|ctx| {
            ctx.intent = self.intent.clone();
            ctx.momentum_tier = self.momentum.tier;
            ctx.constraints = self.constraints.clone();
            ctx.guard_mode = self.constraints.guard_mode;
            ctx.operational_count = self.operational_count();
            ctx.member_count = self.members.len();
            ctx.paused = self.paused;
        });
    }

    /// Broadcast a peer event to all formation members.
    pub fn broadcast_peer(&self, msg: PeerMsg) {
        let _ = self.peer_tx.send(msg);
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

    /// Add a member to a live formation (spec §6.3).
    ///
    /// Registers the agent in the attention economy, pushes the member,
    /// broadcasts `PeerMsg::Joined` so all existing members see the join,
    /// and updates the shared context with the new member count.
    pub fn join(&mut self, member: FormationMember) {
        let id = member.agent_id;
        self.attention_broker.add_agent(id);
        self.members.push(member);
        self.broadcast_peer(PeerMsg::Joined(id));
        self.broadcast_context();
    }

    /// Remove a member from a live formation (spec §6.3).
    ///
    /// Unregisters from attention economy, removes from the member list,
    /// broadcasts `PeerMsg::Left`, and updates context.
    pub fn leave(&mut self, agent_id: AgentId) {
        self.attention_broker.remove_agent(&agent_id);
        self.members.retain(|m| m.agent_id != agent_id);
        self.broadcast_peer(PeerMsg::Left(agent_id));
        self.broadcast_context();
    }

    /// Subscribe to both the peer event bus and the shared context watch
    /// channel. Per spec §6.3: single call returns both receivers so a
    /// new member can start observing immediately after join().
    pub fn subscribe(
        &self,
    ) -> (
        broadcast::Receiver<PeerMsg>,
        watch::Receiver<FormationContext>,
    ) {
        (self.peer_tx.subscribe(), self.context_tx.subscribe())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_member(cap: &str) -> FormationMember {
        FormationMember::new(AgentId::new(), vec![cap.into()])
    }

    fn constraints_with_fuel(fuel: u64) -> FormationConstraints {
        FormationConstraints {
            fuel_budget: springtale_cooperation::FuelAmount(fuel),
            ..Default::default()
        }
    }

    #[test]
    fn test_formation_creation() {
        let formation = Formation::new(
            vec![test_member("slack"), test_member("github")],
            IntentPattern::Reconnoiter {
                target: "issues".to_owned(),
            },
            constraints_with_fuel(10_000),
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
            constraints_with_fuel(1000),
        );
        assert!(formation.is_viable());

        formation.members[0].health = AgentHealth::Dead { recoverable: false };
        assert!(!formation.is_viable());
    }

    #[test]
    fn join_adds_member_and_broadcasts() {
        let mut formation = Formation::new(
            vec![test_member("slack")],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let (mut peer_rx, mut ctx_rx) = formation.subscribe();
        let new_member = test_member("github");
        let new_id = new_member.agent_id;

        formation.join(new_member);

        assert_eq!(formation.members.len(), 2);
        assert!(formation.member(&new_id).is_some());

        // Verify PeerMsg::Joined was broadcast
        let msg = peer_rx.try_recv().expect("should receive join msg");
        assert!(matches!(msg, PeerMsg::Joined(id) if id == new_id));

        // Verify context was updated with new member count
        assert!(ctx_rx.has_changed().unwrap_or(false));
        let ctx = ctx_rx.borrow_and_update();
        assert_eq!(ctx.member_count, 2);
    }

    #[test]
    fn leave_removes_member_and_broadcasts() {
        let m1 = test_member("slack");
        let m2 = test_member("github");
        let leave_id = m2.agent_id;
        let mut formation = Formation::new(
            vec![m1, m2],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let (mut peer_rx, mut ctx_rx) = formation.subscribe();

        formation.leave(leave_id);

        assert_eq!(formation.members.len(), 1);
        assert!(formation.member(&leave_id).is_none());

        // Verify PeerMsg::Left was broadcast
        let msg = peer_rx.try_recv().expect("should receive leave msg");
        assert!(matches!(msg, PeerMsg::Left(id) if id == leave_id));

        // Verify context was updated
        assert!(ctx_rx.has_changed().unwrap_or(false));
        let ctx = ctx_rx.borrow_and_update();
        assert_eq!(ctx.member_count, 1);
    }

    #[test]
    fn subscribe_returns_both_channels() {
        let formation = Formation::new(
            vec![test_member("slack")],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let (peer_rx, ctx_rx) = formation.subscribe();

        // Both receivers are functional — verify they can borrow/recv
        assert!(peer_rx.is_empty());
        let ctx = ctx_rx.borrow();
        assert_eq!(ctx.member_count, 1);
    }
}
