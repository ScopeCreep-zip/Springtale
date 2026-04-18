//! Formation blackboard — composed shared workspace.
//!
//! Per Hayes-Roth blackboard model: knowledge sources post incremental
//! solutions to a shared data structure. Our blackboard composes
//! key-value state, task routing, and result collection.
//!
//! Moved from `orchestrator/coordinator.rs` per M8 — the blackboard
//! is a cooperation primitive, not an orchestration detail.

pub mod cooperative;
pub mod trait_;

pub use cooperative::{BlackboardEntry, BlackboardOp, CooperativeBlackboard};
pub use trait_::Blackboard;
