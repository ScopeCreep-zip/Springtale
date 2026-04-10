//! Cooperative agent architecture — game-informed multi-agent coordination.
//!
//! Per COOPERATION.pdf: "Game developers are the most experienced
//! practitioners of multi-agent cooperative systems in existence.
//! This document treats game designs as engineering references,
//! not analogies."
//!
//! This module handles everything between intent and outcome.
//! The orchestrator (§3) owns composition, intent, constraints,
//! and intervention. Cooperation owns task decomposition, timing
//! coordination, role adaptation, information fusion, failure
//! recovery, and resource allocation within constraints.

pub mod action;
pub mod cadence;
pub mod formation;
pub mod momentum;

pub mod awareness;
pub mod attention;

pub mod environment;
pub mod comms;

pub mod consensus;
pub mod commit;

pub mod interference;
pub mod transformation;

pub mod rally;
pub mod recovery;

pub mod capability;
pub mod handoff;
pub mod mental_model;
pub mod pacing;
pub mod sacrifice;

pub use cadence::{AgentId, CadenceBus, IntentPattern, Tick, TickReport};
pub use formation::{
    AgentHealth, DynamicRole, Formation, FormationConstraints, FormationId, FormationMember,
};
pub use momentum::{MomentumState, MomentumTier};
pub use awareness::{LocalAwareness, NeighborSnapshot};
pub use action::{SubTask, SubTaskResult};
pub use attention::AttentionEconomy;
