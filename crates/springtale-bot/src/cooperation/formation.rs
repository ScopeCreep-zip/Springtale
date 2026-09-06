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
use springtale_cooperation::awareness::{GossipStore, InMemoryGossipStore, LocalAwareness};
use springtale_cooperation::cadence::{AgentId, CadenceBus, IntentPattern};
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::commit::CommitBarrier;
use springtale_cooperation::comms::{
    AckDispatch, FormationBus, FormationBusSubscription, ProtocolDispatch,
};
use springtale_cooperation::consensus::ConsensusEngine;
use springtale_cooperation::context::FormationContext;
use springtale_cooperation::handoff::{
    FlexibleChainPool, HandoffResult, HandoffType, dispatch_handoff_durable,
};
use springtale_cooperation::mental_model::SharedMentalModel;
use springtale_cooperation::momentum::{MomentumState, MomentumTier};
use springtale_cooperation::pacing::PacingManager;
use springtale_cooperation::peer::PeerMsg;
use springtale_cooperation::rally::FormationRally;
use springtale_cooperation::role::{DynamicRoleTrait, GeneralAgent};
use springtale_cooperation::routing::direct::DirectInbox;
use springtale_cooperation::state::SharedEnvironment;
use springtale_cooperation::supervision::{FormationSupervisor, Liveness};
use springtale_cooperation::types::{AgentHealth, FormationConstraints, FormationId};
use springtale_store::StorageBackend;

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
    pub last_report_tick: springtale_cooperation::TickId,
    /// Consecutive tick failures for this member. Used by transformation
    /// trigger (§14) — 5+ failures → ToSupportAgent.
    pub consecutive_failures: usize,
    /// Consecutive ticks with no action. Hits `LISTENING_AFTER_TICKS`
    /// exactly once per idle stretch → `UtteranceKind::Listening`.
    pub consecutive_idle_ticks: u32,
    /// The agent's current task with lifecycle tracking.
    /// Per Spring engine: command queue front = current task.
    /// None when agent is idle (monitoring connector).
    pub active_task: Option<springtale_cooperation::action_state::ActiveTask>,
    /// A dispatch that outlived its beat (plan 1.8 / 0.3). The member
    /// reports `Requested` until `act` collects the finished handle;
    /// `supervision` reads `since` against `constraints.timeout`.
    pub pending: crate::cooperation::dispatch_outcome::PendingSlot,
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
            .field("pending", &self.pending.is_some())
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
            last_report_tick: springtale_cooperation::TickId::ZERO,
            consecutive_failures: 0,
            consecutive_idle_ticks: 0,
            active_task: None,
            pending: crate::cooperation::dispatch_outcome::PendingSlot::default(),
            ai_adapter: None,
        }
    }

    /// Create with string capabilities (convenience for backward compat).
    pub fn from_strings(agent_id: AgentId, capabilities: Vec<String>) -> Self {
        let caps: Vec<CapabilityDecl> =
            capabilities.into_iter().map(CapabilityDecl::from).collect();
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
///
/// Per-member breakdown of bus messages consumed by a single call to
/// `Formation::drain_member_subs`. Emitted so observability surfaces
/// can surface "heard N state callouts this tick" without a separate
/// audit path.
#[derive(Debug, Clone)]
pub struct MemberDrainCounts {
    pub agent_id: AgentId,
    pub state: u32,
    pub cohesion: u32,
    pub protocol: u32,
}

pub struct Formation {
    pub id: FormationId,
    pub members: Vec<FormationMember>,
    pub intent: IntentPattern,
    /// When `true`, tick processing skips this formation entirely.
    pub paused: bool,
    pub constraints: FormationConstraints,
    pub momentum: MomentumState,
    /// Hayes-Roth task-routing blackboard (§3 composer output). Distinct
    /// from [`shared_env`] which is the §10 atomic workspace. The two
    /// serve different concerns — `blackboard` carries task posts, claims,
    /// and results; `shared_env` carries surfaces, write-log, and RCU
    /// snapshots for interference analysis.
    pub blackboard: Arc<CooperativeBlackboard>,
    /// §10 shared workspace with surfaces + write-log audit trail. Fed
    /// into the interference detector (§13 ActionNegation needs the log)
    /// and the stigmergy surface reactions (§10 compose_surfaces).
    pub shared_env: Arc<SharedEnvironment>,
    /// Per-formation fuel budget. `Arc`-wrapped so the beat's spawned dispatches
    /// and `BlackboardRouter` can share the same atomic counter — cloning
    /// `FuelBudget` creates a fresh atomic, which would silently fork the
    /// budget. Existing call sites pass `formation.fuel.as_ref()` where
    /// `&FuelBudget` is expected.
    pub fuel: Arc<FuelBudget>,
    /// Formation-level AI orchestrator. Uses the same `AiAdapter` trait
    /// as per-agent adapters (Ollama, OpenAI, Anthropic).
    /// Loaded from config store key `ai:formation:{id}` at deploy time.
    pub orchestrator: Option<Arc<dyn springtale_ai::AiAdapter>>,

    // ── Cooperation subsystem state ──────────────────────────────────
    /// L4D Director-inspired work/rest cycle management (§22).
    pub pacing: PacingManager,
    /// Monster Hunter cart + JoinSet supervision (§15). `RallyTokens`
    /// wraps `Arc<Semaphore>` so the cascade handler and the supervisor
    /// loop share the same budget without a `&mut` bottleneck.
    pub rally: FormationRally,
    /// Zero-sum workload distribution (§9, Army of Two aggro meter).
    /// ArcSwap-backed for concurrent lock-free reads. Wrapped in `Arc` so
    /// the beat's spawned dispatches can read it concurrently with the
    /// tick pipeline. Existing
    /// `formation.attention_broker.current()` call sites still work via
    /// `Arc` Deref.
    pub attention_broker: Arc<AttentionBroker>,
    /// Erlang OTP-style supervision with liveness probes (§M2).
    pub supervisor: FormationSupervisor,
    /// Accumulated cooperative knowledge (§21).
    pub mental_model: SharedMentalModel,
    /// Active consensus votes (§11, As Dusk Falls voting).
    pub consensus: ConsensusEngine,
    /// Tasks with an open consensus vote, keyed by task id → vote id
    /// (B7 guard). The executor skips proposing for a task already here,
    /// so a released-but-unresolved destructive task can't re-open a new
    /// vote every tick. Entries move to `consensus_approved` (approve) or
    /// are dropped (deny/timeout) by `tick_steps/resolve_consensus`.
    pub awaiting_consensus: std::collections::HashMap<uuid::Uuid, uuid::Uuid>,
    /// When each declared sensing poll last ran, keyed by
    /// (member, connector, action). Only actions with
    /// `ActionDecl::poll_interval_secs` are ever scheduled; the formation
    /// never invents a poll for an action that did not ask for one.
    pub poll_schedule: std::collections::HashMap<(AgentId, String, String), std::time::Instant>,
    /// One-shot execution permits minted by an approving vote resolution.
    /// Consumed (removed) by the executor when the task is claimed, so an
    /// approval authorizes exactly one execution.
    pub consensus_approved: std::collections::HashSet<uuid::Uuid>,
    /// Active synchronized commit barriers (§12, Splinter Cell dual breach).
    pub active_commits: Vec<CommitBarrier>,

    // ── Communication (§6, §19) ─────────────────────────────────
    /// Multi-layer communication bus for inter-agent messaging (§19).
    pub bus: FormationBus,
    /// Handle to the spawned protocol fan-out dispatcher task. `None`
    /// until `lifecycle::spawn_formation` starts the task; aborted on
    /// dissolve to release the ProtocolDispatch receiver.
    pub protocol_dispatcher: Option<tokio::task::JoinHandle<()>>,
    /// Handle to the spawned intent-ack consumer task. Same lifecycle
    /// as `protocol_dispatcher`.
    pub ack_dispatcher: Option<tokio::task::JoinHandle<()>>,
    /// Peer event broadcast — structural events (join/leave/down).
    pub peer_tx: broadcast::Sender<PeerMsg>,
    /// Shared context watch — intent, momentum, constraints broadcast to all members.
    pub context_tx: watch::Sender<FormationContext>,

    // ── Shared handles wired by the runtime (§17 lifecycle) ────────────
    /// External tick bus (spec §5). `Arc`-shared so many formations can
    /// subscribe to the same drumbeat without cloning the bus itself.
    pub cadence: Arc<CadenceBus>,
    /// Workspace-level persistence. Backs §13 CAS, §20 durable deposits,
    /// and §21 shared-mental-model save/load. `Arc<dyn>` so callers
    /// inject any `StorageBackend` (SQLite in prod, in-memory in tests).
    pub store: Arc<dyn StorageBackend>,
    /// §8 gossip substrate. Single-process deployments inject
    /// `InMemoryGossipStore`; cross-process injects `ChitchatGossipStore`.
    pub gossip_store: Arc<dyn GossipStore>,
    /// G6 — cross-formation gossip bus. Optional so formations created
    /// outside the bot runtime (CLI dry-runs, tests) don't have to wire
    /// up the full chitchat substrate. When `Some`, `tick_steps::
    /// publish_formation_view` broadcasts this formation's running
    /// state every tick and `lifecycle::dissolve` publishes a terminal
    /// `FormationOutcome`.
    pub formation_gossip: Option<Arc<dyn springtale_cooperation::gossip::FormationGossipBus>>,
    /// §20 FlexibleChain work-stealing pool — per-capability crossbeam
    /// deques. One instance per formation so the capability scope is
    /// bounded and steal-miss iteration stays cheap at RTS scale.
    pub flex_chain_pool: Arc<FlexibleChainPool>,
    /// §20 Direct-handoff inbox — per-receiver FIFO queues used by
    /// `HandoffType::Direct`. Populated by `dispatch_handoff_durable`;
    /// drained by member runner tasks that pull their assigned tasks
    /// from the inbox matching their own `AgentId`.
    pub direct_inbox: Arc<DirectInbox>,

    /// Per-member bus subscriptions keyed by AgentId. Populated at
    /// `Formation::new` and `join()` time; consumed one-at-a-time by
    /// member runner tasks via `take_subscription`. Protected by
    /// `std::sync::Mutex` because `FormationBusSubscription` contains
    /// an `mpsc::Receiver` which is `Send` but not `Clone` — removal
    /// must be atomic w.r.t. concurrent member churn.
    pub member_subs: std::sync::Mutex<std::collections::HashMap<AgentId, FormationBusSubscription>>,
    /// Tick the formation is currently processing (set by `run_tick`);
    /// utterances raised outside the executor are stamped with it.
    pub current_tick: springtale_cooperation::TickId,
    /// Stardew `blockedIntervalBeforeEmote` bookkeeping per `(agent, kind)`.
    pub last_uttered: springtale_cooperation::utterance::emit::LastUttered,
    /// Utterance def table (plan §1.15) — default until config overrides land.
    pub utterance_defs: Arc<springtale_cooperation::utterance::UtteranceDefs>,

    /// Wall-clock instant of the last PROCESSED tick (after the §22
    /// pacing divider). `None` until the first processed tick. Used to
    /// compute the true per-formation elapsed time for pacing — the
    /// divider makes "one bus tick" a wrong unit, and `tick.window`
    /// (the agent commit window, interval ×4) always was.
    pub last_tick_at: Option<std::time::Instant>,
    /// Cursor into `shared_env.snapshot().write_log` — number of entries
    /// consumed by the last tick's interference pass. Writes at index
    /// ≥ this cursor on the next tick are "current-tick" writes; earlier
    /// entries are "history" (§13 ActionNegation detection requires the
    /// split).
    pub last_tick_write_count: usize,
    /// Momentum tier as of the last broadcast. When the current tier
    /// differs, the event loop emits a CohesionSignal to the bus so
    /// subscribers learn the cadence shifted (spec §19 Rock-and-Stone
    /// pattern). Initialized to Cold so the first promotion fires
    /// exactly once.
    pub last_broadcast_tier: MomentumTier,

    /// Number of consecutive ticks where `cascade::detect_cascade`
    /// returned `Some(_)`. Reset on `all_succeeded`. Drives the
    /// `cascade_hits` signal that the L6 intervention layer reads
    /// (`tick_steps/check_interventions.rs`) — high streaks trigger
    /// `Stabilize` intent change or `ForcedDissolve` per
    /// `crates/springtale-bot/src/orchestrator/intervention/evaluator/rules.rs`.
    pub cascade_hit_streak: u32,
    /// Set by `tick_steps/supervision.rs` when the supervisor returns
    /// `SupervisionAction::Escalate`. Read by `check_interventions.rs`
    /// next tick and folded into the intervention signals; cleared after
    /// the intervention layer consumes it.
    pub escalation_pending: Option<String>,
    /// Set by `tick_steps/supervision.rs` when the supervisor returns
    /// `SupervisionAction::TriggerReplan`. Read by the L5 CBBA executor
    /// (B3) which performs the replan and clears the flag. Surfaces in
    /// the L6 intervention signals as `cbba_stalled` while it remains
    /// set across multiple ticks without resolution.
    pub needs_replan: bool,

    /// L4 Contract Net broadcast channels (`COOPERATION.md §11`/§L4).
    /// Held at formation scope so all members receive the same CFP set;
    /// arrivals are drained into `open_cfps` at the top of each beat.
    pub cfp_channels: springtale_cooperation::contract_net::CfpChannels,
    /// Initiator end of the CFP channels — owns the bid receiver.
    /// Wrapped in `tokio::Mutex` because `coordinator::run_round` borrows
    /// it `&mut` and CFP rounds may be initiated from any tick step.
    pub cfp_initiator:
        Arc<tokio::sync::Mutex<springtale_cooperation::contract_net::InitiatorHandle>>,
    /// CFPs that arrived since the last beat, drained from `cfp_channels`
    /// at the top of the beat (`drain_open_cfps`). The decide phase
    /// answers the first one per member via `respond_cfp` (plan 1.9).
    pub open_cfps: Vec<springtale_cooperation::contract_net::types::CallForProposals>,
    /// Formation-scoped CFP receiver feeding `open_cfps`.
    pub cfp_rx: broadcast::Receiver<springtale_cooperation::contract_net::types::CallForProposals>,
    /// One shared bidder for every member (plan 1.8): scores against the
    /// bidding member's `AgentContext` capabilities.
    pub bidder: Arc<dyn springtale_cooperation::contract_net::trait_::Bidder>,

    /// L0 stigmergy substrate (`COOPERATION.md §10`). Trait-object so the
    /// concrete backend (production `SurfaceStore`, in-test mocks) is
    /// swappable per plan §B4. Agents deposit `Success` / `Substrate`
    /// surfaces after completed actions and read primed surfaces during
    /// `agent/step/sense`. Decay sweep runs at the head of
    /// `tick_steps/build_reports::run` so expired surfaces stop showing up.
    pub surfaces: Arc<dyn springtale_cooperation::stigmergy::SurfaceSubstrate>,

    /// L1 routine task router (B5). Bridges the concrete blackboard to the
    /// `TaskRouter` trait so `agent/step/scan` pulls work through one
    /// canonical interface — tier-gated and capability-filtered. The
    /// beat's decide phase (`tick_steps/build_reports/agent_pipeline`)
    /// calls `scan` for the highest-priority match.
    pub task_router: Arc<crate::cooperation::blackboard_router::BlackboardRouter>,
}

/// Shared dependencies injected into every Formation. Constructed once by
/// the runtime (for all formations it spawns) and passed through
/// [`Formation::new`]; tests use [`FormationDeps::test_defaults`] to get a
/// self-contained, in-memory set.
pub struct FormationDeps {
    pub cadence: Arc<CadenceBus>,
    pub store: Arc<dyn StorageBackend>,
    pub gossip_store: Arc<dyn GossipStore>,
    pub flex_chain_pool: Arc<FlexibleChainPool>,
    /// G6 — cross-formation gossip bus. `None` for tests / dry-runs that
    /// don't need cross-formation visibility; production wires the
    /// shared `InMemoryFormationGossipBus` (or chitchat-backed) here.
    pub formation_gossip: Option<Arc<dyn springtale_cooperation::gossip::FormationGossipBus>>,
}

impl FormationDeps {
    /// Self-contained defaults for tests and single-formation scenarios.
    /// Production callers (`bot::cooperation::lifecycle::spawn_formation`)
    /// construct a `FormationDeps` from the bot's handles to the shared
    /// `RuntimeState`, so every formation in the process points at the
    /// same `CadenceBus`, the same `StorageBackend`, and the same
    /// `GossipStore`.
    pub fn test_defaults() -> Self {
        let (cadence, _reports_rx) = CadenceBus::default_30hz();
        Self {
            cadence: Arc::new(cadence),
            store: Arc::new(springtale_store::backend::InMemoryBackend::new()),
            gossip_store: Arc::new(InMemoryGossipStore::new()),
            flex_chain_pool: Arc::new(FlexibleChainPool::new()),
            formation_gossip: None,
        }
    }
}

impl Formation {
    /// Create a new formation. Returns the Formation plus the two bus
    /// dispatcher ends; `lifecycle::spawn_formation` spawns dispatcher
    /// tasks with them and stores the resulting `JoinHandle`s back on
    /// `Formation::protocol_dispatcher` / `ack_dispatcher`.
    ///
    /// Per Total War supply model: fuel is a formation-level constraint set
    /// at composition time. `FuelBudget` is created from
    /// `constraints.fuel_budget` so there's one source of truth for the
    /// initial allocation.
    pub fn new(
        members: Vec<FormationMember>,
        intent: IntentPattern,
        constraints: FormationConstraints,
        deps: FormationDeps,
    ) -> (Self, ProtocolDispatch, AckDispatch) {
        let fuel = Arc::new(FuelBudget::new(constraints.fuel_budget.0));
        let blackboard = Arc::new(CooperativeBlackboard::new());
        let direct_inbox = Arc::new(DirectInbox::new());
        // Note: B6 wires the FlexibleChainPool reference through the router
        // so `agent::step::inbox` can `try_steal_chain` per capability. The
        // Formation field is the same Arc — a single shared pool per
        // formation, populated by `dispatch_handoff` writes and consumed
        // by router reads.
        let task_router = Arc::new(
            crate::cooperation::blackboard_router::BlackboardRouter::new(
                blackboard.clone(),
                fuel.clone(),
                direct_inbox.clone(),
                deps.flex_chain_pool.clone(),
            ),
        );
        let agent_ids: Vec<AgentId> = members.iter().map(|m| m.agent_id).collect();
        let (bus, proto_dispatch, ack_dispatch) = FormationBus::new();
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

        // Per spec §15.3: default to 3 carts (Monster Hunter quest-fail
        // threshold). `FormationConstraints` doesn't currently carry a
        // per-formation override; adding one is a separate capability
        // change and doesn't belong in this phase.
        let rally_budget: usize = 3;

        // Subscribe every member to the bus up front AND register them
        // in the flex-chain pool for every capability they declare.
        // - Bus: dispatcher resolves `Specific(agent_id)` targets by
        //   DashMap lookup, so the inbox must exist before the first tick.
        // - Flex chain (§20.4): FlexibleChain handoffs post into the
        //   per-capability Injector; only registered Workers can claim.
        let mut initial_subs = std::collections::HashMap::new();
        for member in &members {
            initial_subs.insert(member.agent_id, bus.subscribe(member.agent_id));
            for cap in &member.capabilities {
                deps.flex_chain_pool.register(cap.clone(), member.agent_id);
            }
        }

        // L4 Contract Net channel set — created once per formation. The
        // `CfpChannels` half is shared (cloned senders, fresh participant
        // receivers); the `InitiatorHandle` half holds the bid_rx and is
        // borrowed `&mut` by `coordinator::run_round`.
        let (cfp_channels, cfp_initiator_inner) =
            springtale_cooperation::contract_net::CfpChannels::new();
        let cfp_initiator = Arc::new(tokio::sync::Mutex::new(cfp_initiator_inner));
        let cfp_rx = cfp_channels.cfp_tx.subscribe();

        let formation = Self {
            id: FormationId::new(),
            intent,
            paused: false,
            constraints,
            momentum: MomentumState::default(),
            blackboard,
            shared_env: Arc::new(SharedEnvironment::new()),
            fuel,
            orchestrator: None,
            pacing: PacingManager::default(),
            rally: FormationRally::new(rally_budget, 64),
            attention_broker: Arc::new(AttentionBroker::for_agents(&agent_ids)),
            supervisor: FormationSupervisor::default(),
            mental_model: SharedMentalModel::default(),
            consensus: ConsensusEngine::new(),
            awaiting_consensus: std::collections::HashMap::new(),
            poll_schedule: std::collections::HashMap::new(),
            consensus_approved: std::collections::HashSet::new(),
            active_commits: Vec::new(),
            bus,
            protocol_dispatcher: None,
            ack_dispatcher: None,
            peer_tx,
            context_tx,
            members,
            cadence: deps.cadence,
            store: deps.store,
            gossip_store: deps.gossip_store,
            formation_gossip: deps.formation_gossip,
            flex_chain_pool: deps.flex_chain_pool,
            direct_inbox,
            member_subs: std::sync::Mutex::new(initial_subs),
            current_tick: springtale_cooperation::TickId::ZERO,
            last_uttered: springtale_cooperation::utterance::emit::LastUttered::new(),
            utterance_defs: Arc::new(springtale_cooperation::utterance::UtteranceDefs::default()),
            last_tick_at: None,
            last_tick_write_count: 0,
            last_broadcast_tier: MomentumTier::Cold,
            cascade_hit_streak: 0,
            escalation_pending: None,
            cfp_channels,
            cfp_initiator,
            open_cfps: Vec::new(),
            cfp_rx,
            bidder: Arc::new(springtale_cooperation::contract_net::bid::evaluate::ContextBidder),
            surfaces: Arc::new(springtale_cooperation::stigmergy::deposit::SurfaceStore::new())
                as Arc<dyn springtale_cooperation::stigmergy::SurfaceSubstrate>,
            task_router,
            needs_replan: false,
        };
        (formation, proto_dispatch, ack_dispatch)
    }

    /// Constructor that discards the bus dispatcher ends — intended for
    /// tests and tooling that exercise intent / rally / attention /
    /// momentum / broadcast / watch behavior but do NOT rely on
    /// cross-agent protocol delivery or intent-ack fan-in.
    ///
    /// Produces a formation whose protocol-message fan-out and intent-ack
    /// fan-in are not routed — sends succeed until the mpsc buffer fills,
    /// then block. Production code paths (`lifecycle::spawn_formation`)
    /// must use `Formation::new` and spawn the dispatcher tasks.
    pub fn new_disconnected(
        members: Vec<FormationMember>,
        intent: IntentPattern,
        constraints: FormationConstraints,
    ) -> Self {
        let (formation, _proto, _ack) =
            Self::new(members, intent, constraints, FormationDeps::test_defaults());
        formation
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

    /// Open a synchronized-commit barrier (§12). Every listed participant
    /// must call `signal_commit_ready` before the deadline; if they
    /// don't, the barrier expires and the next tick's drain drops it.
    /// Returns the barrier id so callers can correlate later
    /// `signal_commit_ready` / `record_commit_result` calls.
    ///
    /// Hot+ tier requirement is enforced at the caller — momentum
    /// gating lives in `momentum::MomentumState::can_synchronized_commit`.
    pub fn begin_commit(
        &mut self,
        participants: &[AgentId],
        deadline: std::time::Duration,
        initiated_by: AgentId,
    ) -> uuid::Uuid {
        let barrier = springtale_cooperation::commit::CommitBarrier::new(
            participants,
            deadline,
            initiated_by,
        );
        let id = barrier.id;
        self.active_commits.push(barrier);
        id
    }

    /// Mark a participant ready on an active commit barrier. Returns
    /// `Ok(())` on success, error if the barrier is unknown, past
    /// prepare, or the agent wasn't listed in `begin_commit`.
    pub fn signal_commit_ready(
        &mut self,
        barrier_id: uuid::Uuid,
        agent: AgentId,
    ) -> Result<(), springtale_cooperation::error::CommitError> {
        let barrier = self
            .active_commits
            .iter_mut()
            .find(|b| b.id == barrier_id)
            .ok_or_else(|| {
                springtale_cooperation::error::CommitError::BarrierFailed(format!(
                    "unknown barrier {barrier_id}"
                ))
            })?;
        barrier.signal_ready(agent)
    }

    /// Record a participant's sub-task result against an active
    /// barrier. Transitions the barrier into `Collect` once every
    /// participant has reported (success or failure).
    pub fn record_commit_result(
        &mut self,
        barrier_id: uuid::Uuid,
        agent: AgentId,
        result: springtale_cooperation::SubTaskResult,
    ) {
        if let Some(barrier) = self.active_commits.iter_mut().find(|b| b.id == barrier_id) {
            barrier.collect_result(agent, result);
        }
    }

    /// Number of currently-open commit barriers. Exposed so tests and
    /// observability surfaces can verify the commit path is live.
    pub fn active_commit_count(&self) -> usize {
        self.active_commits.len()
    }

    /// Top of the beat: move every CFP that arrived since the last beat
    /// into `open_cfps` (plan 1.8 / 1.9). Nothing is answered here; the
    /// decide phase bids per member against the same list.
    pub fn drain_open_cfps(&mut self) {
        self.open_cfps.clear();
        loop {
            match self.cfp_rx.try_recv() {
                Ok(cfp) => self.open_cfps.push(cfp),
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(formation = %self.id.0, skipped, "cfp receiver lagged");
                }
                Err(_) => break,
            }
        }
    }

    /// Add a member to a live formation (spec §6.3).
    ///
    /// Registers the agent in the attention economy, pushes the member,
    /// broadcasts `PeerMsg::Joined` so all existing members see the join,
    /// and updates the shared context with the new member count. The new
    /// member bids on open CFPs from its first beat (plan 1.9).
    pub fn join(&mut self, member: FormationMember) {
        let id = member.agent_id;
        // Subscribe the new member to the bus first — the dispatcher
        // resolves protocol targets by DashMap lookup on agent_id, so a
        // `Specific(id)` message arriving between join and subscribe
        // would be dropped with "target not subscribed".
        let sub = self.bus.subscribe(id);
        if let Ok(mut guard) = self.member_subs.lock() {
            guard.insert(id, sub);
        }
        // Register the member as a worker for every capability they
        // declare (spec §20.4). FlexibleChain handoffs post into a
        // per-capability Injector, and stealing requires the target
        // agent to have a Worker registered for that capability.
        for cap in &member.capabilities {
            self.flex_chain_pool.register(cap.clone(), id);
        }
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
        // Drop the per-member subscription and remove the inbox from the
        // bus's routing table so future `Specific(agent_id)` protocol
        // messages don't block on a dead receiver.
        if let Ok(mut guard) = self.member_subs.lock() {
            guard.remove(&agent_id);
        }
        self.bus.unsubscribe(agent_id);
        // Unregister from each capability's flex-chain pool so stealers
        // stop polling this agent's (soon-dropped) Worker.
        if let Some(m) = self.members.iter().find(|m| m.agent_id == agent_id) {
            for cap in &m.capabilities {
                self.flex_chain_pool.unregister(cap, agent_id);
            }
        }
        self.attention_broker.remove_agent(&agent_id);
        self.members.retain(|m| m.agent_id != agent_id);
        self.broadcast_peer(PeerMsg::Left(agent_id));
        self.broadcast_context();
    }

    /// Pull a member's bus subscription out of the Formation so a runner
    /// task can own and consume it. Returns `None` if the subscription
    /// was already taken or the agent never subscribed. One-shot per
    /// agent — subsequent calls for the same agent return `None` until a
    /// new `join` repopulates.
    pub fn take_subscription(&self, agent_id: AgentId) -> Option<FormationBusSubscription> {
        self.member_subs
            .lock()
            .ok()
            .and_then(|mut g| g.remove(&agent_id))
    }

    /// Drain any pending bus messages that have accumulated in each
    /// member's subscription since the last tick. Called from the event
    /// loop's per-tick pipeline so the broadcast/mpsc channels don't
    /// backlog indefinitely — the agent-runner tasks that will
    /// eventually *own* these subscriptions don't exist yet, and in the
    /// meantime `try_recv` + log keeps the bus live.
    ///
    /// Returns per-member counts `{ state, cohesion, protocol }` for
    /// observability: the formation UI surface can render "heard N
    /// state callouts this tick" without a separate audit table.
    /// Borrow the disjoint fields `utterance::utter` needs, stamped with
    /// `current_tick`. Callers with a live `Tick` use [`Self::utter_ctx_at`].
    pub fn utter_ctx<'a>(
        &'a mut self,
        tx: Option<&'a broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>>,
    ) -> springtale_cooperation::utterance::UtterCtx<'a> {
        let tick = self.current_tick;
        self.utter_ctx_at(tick, tx)
    }

    pub fn utter_ctx_at<'a>(
        &'a mut self,
        tick: springtale_cooperation::TickId,
        tx: Option<&'a broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>>,
    ) -> springtale_cooperation::utterance::UtterCtx<'a> {
        springtale_cooperation::utterance::UtterCtx {
            formation_id: self.id,
            bus: &self.bus,
            defs: &self.utterance_defs,
            last_uttered: &mut self.last_uttered,
            tick,
            tx,
        }
    }

    pub fn drain_member_subs(&self) -> Vec<MemberDrainCounts> {
        use tokio::sync::broadcast::error::TryRecvError as BroadcastTry;
        use tokio::sync::mpsc::error::TryRecvError as MpscTry;

        let Ok(mut subs) = self.member_subs.lock() else {
            return Vec::new();
        };
        let mut counts = Vec::with_capacity(subs.len());
        for (agent_id, sub) in subs.iter_mut() {
            let mut state = 0u32;
            let mut cohesion = 0u32;
            let mut protocol = 0u32;
            loop {
                match sub.state_rx.try_recv() {
                    Ok(msg) => {
                        state += 1;
                        tracing::trace!(
                            agent = %agent_id.0,
                            ?msg.trigger,
                            "bus drain: state broadcast"
                        );
                    }
                    Err(BroadcastTry::Empty) => break,
                    Err(BroadcastTry::Lagged(n)) => {
                        tracing::warn!(
                            agent = %agent_id.0,
                            skipped = n,
                            "state_rx lagged"
                        );
                    }
                    Err(BroadcastTry::Closed) => break,
                }
            }
            loop {
                match sub.cohesion_rx.try_recv() {
                    Ok(_) => cohesion += 1,
                    Err(BroadcastTry::Empty) => break,
                    Err(BroadcastTry::Lagged(n)) => {
                        tracing::warn!(
                            agent = %agent_id.0,
                            skipped = n,
                            "cohesion_rx lagged"
                        );
                    }
                    Err(BroadcastTry::Closed) => break,
                }
            }
            loop {
                match sub.protocol_rx.try_recv() {
                    Ok(msg) => {
                        protocol += 1;
                        tracing::trace!(
                            agent = %agent_id.0,
                            source = %msg.source.0,
                            ?msg.payload,
                            "bus drain: protocol msg"
                        );
                    }
                    Err(MpscTry::Empty) => break,
                    Err(MpscTry::Disconnected) => break,
                }
            }
            counts.push(MemberDrainCounts {
                agent_id: *agent_id,
                state,
                cohesion,
                protocol,
            });
        }
        counts
    }

    /// Subscribe to the shared cadence bus (spec §5). Returns a new tick
    /// receiver — agent-level code uses this to drive the perceive →
    /// decide → act → report loop on the formation's drumbeat. The
    /// underlying bus is `Arc`-shared across every formation in the
    /// process, so all formations see the same tick sequence.
    pub fn subscribe_cadence(
        &self,
    ) -> tokio::sync::broadcast::Receiver<springtale_cooperation::cadence::Tick> {
        self.cadence.subscribe()
    }

    /// Current cadence tick interval (spec §5). Exposed for members that
    /// need to size their own timeouts relative to the drumbeat rate.
    pub fn tick_interval(&self) -> std::time::Duration {
        self.cadence.tick_interval
    }

    /// Route a handoff onto the formation's Phase-K substrates. Direct
    /// handoffs go to `direct_inbox`, EnvironmentMediated to the durable
    /// `coop_deposits` table via `store`, FlexibleChain to the per-
    /// capability `flex_chain_pool` (work-stealing), and Sequential /
    /// InformationTransfer are logical (no substrate).
    ///
    /// Per spec §20 this is THE handoff API exposed to agent
    /// behaviors. Agents never touch the substrates directly — they
    /// construct a `HandoffType` and call this method, which guarantees
    /// the durable + work-stealing semantics and wires TTL from
    /// [`FormationConstraints::timeout`].
    pub async fn dispatch_handoff(
        &self,
        handoff: &HandoffType,
    ) -> Result<HandoffResult, springtale_cooperation::error::CooperationError> {
        let ttl = Some(self.constraints.timeout);
        dispatch_handoff_durable(
            handoff,
            &self.store,
            &self.flex_chain_pool,
            Some(&self.direct_inbox),
            ttl,
        )
        .await
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

impl Drop for Formation {
    /// Abort the bus dispatcher tasks when the formation is dropped.
    /// `tokio::task::JoinHandle::drop` does NOT cancel the task by itself,
    /// so without this Drop impl a dissolved formation would leak its two
    /// dispatcher tasks (protocol + ack) indefinitely. A member dispatch
    /// still in flight (`FormationMember::pending`) is left to finish: a
    /// committed connector action is not cancelled by dissolving the
    /// formation (Splinter Cell: failure after commit exposes both).
    fn drop(&mut self) {
        if let Some(h) = self.protocol_dispatcher.take() {
            h.abort();
        }
        if let Some(h) = self.ack_dispatcher.take() {
            h.abort();
        }
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
        let formation = Formation::new_disconnected(
            vec![test_member("slack"), test_member("github")],
            IntentPattern::Reconnoiter {
                target: "issues".into(),
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
        let mut formation = Formation::new_disconnected(
            vec![test_member("slack")],
            IntentPattern::Stabilize {
                reason: "test".into(),
            },
            constraints_with_fuel(1000),
        );
        assert!(formation.is_viable());

        formation.members[0].health = AgentHealth::Dead { recoverable: false };
        assert!(!formation.is_viable());
    }

    #[test]
    fn join_adds_member_and_broadcasts() {
        let mut formation = Formation::new_disconnected(
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
        let mut formation = Formation::new_disconnected(
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
        let formation = Formation::new_disconnected(
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

    #[test]
    fn take_subscription_returns_populated_inbox() {
        // Every member subscribed at Formation::new gets an entry in
        // member_subs. take_subscription removes and returns it exactly
        // once — subsequent calls for the same agent_id return None.
        let member = test_member("slack");
        let agent = member.agent_id;
        let formation = Formation::new_disconnected(
            vec![member],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let first = formation.take_subscription(agent);
        assert!(first.is_some(), "first take should return the subscription");
        let first_sub = first.unwrap();
        assert_eq!(first_sub.agent_id, agent);
        assert!(
            formation.take_subscription(agent).is_none(),
            "second take should return None — subscription is one-shot"
        );
    }

    #[tokio::test]
    async fn dispatch_handoff_routes_direct_to_inbox() {
        // Exercises Formation::dispatch_handoff end-to-end: the
        // HandoffType flows through dispatch_handoff_durable with the
        // formation's direct_inbox substrate. A Direct handoff lands
        // in the inbox under the receiver's AgentId.
        use springtale_cooperation::handoff::{HandoffPayload, HandoffType};

        let sender = AgentId::new();
        let receiver = AgentId::new();
        let sender_member = FormationMember::new(sender, vec!["github".into()]);
        let receiver_member = FormationMember::new(receiver, vec!["slack".into()]);
        let formation = Formation::new_disconnected(
            vec![sender_member, receiver_member],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );

        let handoff = HandoffType::Direct {
            sender,
            receiver,
            payload: HandoffPayload {
                data: serde_json::json!({"ticket": 42}),
                schema: "slack".to_owned(),
                produced_by: springtale_cooperation::cadence::ActionDescriptor {
                    kind: "producer".to_owned(),
                    target: None,
                    payload_hash: 0,
                },
                consumable_by: vec!["slack".into()],
                expires: None,
            },
        };

        let result = formation
            .dispatch_handoff(&handoff)
            .await
            .expect("dispatch_handoff");
        match result {
            springtale_cooperation::handoff::HandoffResult::Delivered { from, to, .. } => {
                assert_eq!(from, sender);
                assert_eq!(to, receiver);
            }
            other => panic!("expected Delivered, got {other:?}"),
        }
        assert_eq!(formation.direct_inbox.len(receiver), 1);
    }

    #[tokio::test]
    async fn subscribe_cadence_yields_a_live_tick_receiver() {
        // Proves Formation::subscribe_cadence wires to the shared
        // CadenceBus — takes a receiver, publishes a tick via the bus's
        // test entry, and asserts the receiver observes it.
        use springtale_cooperation::cadence::IntentPattern;
        let formation = Formation::new_disconnected(
            vec![test_member("slack")],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let _rx = formation.subscribe_cadence();
        // tick_interval is also exposed — verify it matches the
        // 30hz default the test_defaults factory configures.
        assert_eq!(
            formation.tick_interval(),
            std::time::Duration::from_millis(33)
        );
    }

    #[tokio::test]
    async fn dispatch_handoff_flex_chain_posts_to_registered_pool() {
        // FlexibleChain handoff: the receiver must have registered a
        // Worker for the next_capability_required for find_task to
        // resolve. Formation::new registers every member for their
        // declared capabilities, so this test proves the wire.
        use springtale_cooperation::cadence::ActionDescriptor;
        use springtale_cooperation::handoff::{HandoffPayload, HandoffType};

        let originator = AgentId::new();
        let receiver = AgentId::new();
        let originator_m = FormationMember::new(originator, vec!["producer".into()]);
        // receiver declares "consumer" capability — registered at Formation::new.
        let receiver_m = FormationMember::new(receiver, vec!["consumer".into()]);
        let formation = Formation::new_disconnected(
            vec![originator_m, receiver_m],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );

        let handoff = HandoffType::FlexibleChain {
            originator,
            current_step: 0,
            total_steps: 2,
            payload: HandoffPayload {
                data: serde_json::json!({"step": 1}),
                schema: "producer_out".to_owned(),
                produced_by: ActionDescriptor {
                    kind: "produce".to_owned(),
                    target: None,
                    payload_hash: 0,
                },
                consumable_by: vec!["consumer".into()],
                expires: None,
            },
            next_capability_required: "consumer".into(),
        };

        formation.dispatch_handoff(&handoff).await.unwrap();

        // Receiver should be able to claim the payload from the pool.
        let found = formation
            .flex_chain_pool
            .find_task(&"consumer".into(), receiver);
        assert!(
            found.is_some(),
            "registered worker should find the posted payload"
        );
    }

    #[test]
    fn begin_commit_registers_a_barrier() {
        let a = AgentId::new();
        let b = AgentId::new();
        let mut formation = Formation::new_disconnected(
            vec![
                FormationMember::new(a, vec!["slack".into()]),
                FormationMember::new(b, vec!["github".into()]),
            ],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        assert_eq!(formation.active_commit_count(), 0);

        let id = formation.begin_commit(&[a, b], std::time::Duration::from_secs(5), a);
        assert_eq!(formation.active_commit_count(), 1);
        assert!(formation.active_commits.iter().any(|c| c.id == id));
    }

    #[test]
    fn signal_commit_ready_transitions_barrier() {
        let a = AgentId::new();
        let b = AgentId::new();
        let mut formation = Formation::new_disconnected(
            vec![
                FormationMember::new(a, vec!["slack".into()]),
                FormationMember::new(b, vec!["github".into()]),
            ],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let id = formation.begin_commit(&[a, b], std::time::Duration::from_secs(5), a);
        formation.signal_commit_ready(id, a).unwrap();
        formation.signal_commit_ready(id, b).unwrap();

        // Both participants ready → barrier phase auto-advanced to Ready.
        let barrier = formation
            .active_commits
            .iter()
            .find(|c| c.id == id)
            .expect("barrier present");
        assert!(barrier.all_ready());
    }

    #[test]
    fn signal_commit_ready_rejects_unknown_barrier() {
        let mut formation = Formation::new_disconnected(
            vec![FormationMember::new(AgentId::new(), vec!["slack".into()])],
            IntentPattern::Execute { plan_id: None },
            constraints_with_fuel(1000),
        );
        let err = formation
            .signal_commit_ready(uuid::Uuid::new_v4(), AgentId::new())
            .unwrap_err();
        assert!(format!("{err}").contains("unknown barrier"));
    }
}
