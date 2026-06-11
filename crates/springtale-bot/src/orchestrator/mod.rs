//! Orchestrator — scoped to composition, intent, constraints, and intervention.
//!
//! Per COOPERATION.pdf §3: "This section defines what the scoped
//! orchestrator/ module owns. Everything not listed here belongs
//! to cooperation/."

// Existing modules (Phase 1b)
pub mod error;
pub mod fuel;

// Orchestration boundary (§3)
pub mod composer;
pub mod intent;
pub mod intervention;
pub mod orchestrate;

pub use crate::cooperation::blackboard::CooperativeBlackboard;
pub use error::OrchestratorError;
pub use fuel::FuelBudget;
