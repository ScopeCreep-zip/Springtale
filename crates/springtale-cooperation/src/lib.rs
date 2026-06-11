#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod action;
pub mod action_state;
pub mod agent;
pub mod attention;
pub mod authority;
pub mod awareness;
pub mod cadence;
pub mod capability;
pub mod command;
pub mod commit;
pub mod comms;
pub mod consensus;
pub mod context;
pub mod contract_net;
pub mod dissemination;
pub mod error;
pub mod events;
/// Cooperation envelope threaded with every chain dispatch — agent /
/// formation / momentum tier / mode. See [`execution::ExecutionContext`].
pub mod execution;
pub mod gossip;
pub mod handoff;
pub mod interference;
pub mod layer;
pub mod memory;
pub mod mental_model;
pub mod momentum;
pub mod pacing;
pub mod peer;
pub mod rally;
pub mod recovery;
pub mod replan;
pub mod role;
pub mod routing;
pub mod sacrifice;
pub mod state;
pub mod stigmergy;
pub mod supervision;
pub mod tick;
pub mod tick_processor;
pub mod transformation;
pub mod types;
pub mod utility;

// Re-exports — primary public API surface.
pub use action::{SubTask, SubTaskResult};
pub use attention::AttentionEconomy;
pub use awareness::{LocalAwareness, NeighborSnapshot};
pub use cadence::{
    ActionDescriptor, ActionSummary, AgentId, CadenceBus, DissolveReason, IntentPattern, PlanId,
    StabilizeReason, TaskDescriptor, Tick, TickReport,
};
pub use capability::CapabilityDecl;
pub use context::FormationContext;
pub use error::{
    AwarenessError, CadenceError, CommitError, ConsensusError, CooperationError, FormationError,
    HandoffError, InterferenceError, MomentumError, PacingError, RallyError, RecoveryError,
};
pub use events::{
    CooperationEvent, CooperationEventEnvelope, InterferenceKind, InterventionKind,
    ReplanOutcomeSummary, VoteOutcome,
};
pub use momentum::{MomentumState, MomentumTier};
pub use peer::PeerMsg;
pub use tick::TickId;
pub use types::{
    AgentHealth, ApprovalPolicy, AutonomyLevel, DynamicRole, FormationConstraints, FormationId,
    FuelAmount, PatternId, ResourceId, WorkspaceKey,
};
