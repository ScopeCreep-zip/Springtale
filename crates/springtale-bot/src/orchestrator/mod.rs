//! Orchestrator — scoped to composition, intent, constraints, and intervention.
//!
//! Per COOPERATION.pdf §3: "This section defines what the scoped
//! orchestrator/ module owns. Everything not listed here belongs
//! to cooperation/."

// Existing modules (Phase 1b)
pub mod coordinator;
pub mod error;
pub mod fuel;
pub mod recursive;
pub mod subagent;

// Orchestration boundary (§3)
pub mod composer;
pub mod constraints;
pub mod intent;
pub mod intervention;

pub use coordinator::CooperativeBlackboard;
pub use error::OrchestratorError;
pub use fuel::FuelBudget;
pub use recursive::{ChildResult, ChildTask, Orchestrator, OrchestratorConfig};
