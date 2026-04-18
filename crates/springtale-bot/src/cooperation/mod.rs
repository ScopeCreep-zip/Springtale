//! Cooperative agent architecture — game-informed multi-agent coordination.
//!
//! Cooperation primitives live in the dedicated `springtale-cooperation` crate.
//! This module re-exports them for internal use and provides the `Formation`
//! composition type that binds cooperation primitives to orchestrator
//! infrastructure (AiAdapter, CooperativeBlackboard, FuelBudget).

// Re-export all cooperation primitives from the dedicated crate.
pub use springtale_cooperation::*;

// Formation stays here — it's the composition layer binding cooperation
// primitives to orchestrator infrastructure (AiAdapter, Blackboard, Fuel).
pub mod blackboard;
pub mod formation;
pub mod lifecycle;
pub mod task_dispatch;
