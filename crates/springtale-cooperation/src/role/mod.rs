//! Dynamic roles — composable agent decision policies.
//!
//! Per COOPERATION.md §14.4: roles are trait objects with `can_execute`
//! and `capabilities` methods. The agent loop checks `role.can_execute`
//! before dispatching actions — if the role says no, the action is skipped.
//!
//! Per RTS composable FSM: role = high-level policy, behaviors = low-level FSM.
//! Per OpenAI Swarm: handoff = swap active agent's instruction set entirely.
//! Per Spring RTS AllowedCommand: capability check at dispatch time.
//!
//! Three built-in roles: General (full access), Information (read-only after
//! primary loss — Siege dead→cameras), Support (assist-only — Army of Two
//! overwatch). Connectors can define custom roles implementing the trait.

pub mod apply;
pub mod general;
pub mod information;
pub mod support;
pub mod trait_;

pub use apply::{apply_transformation, from_name};
pub use general::GeneralAgent;
pub use information::InformationAgent;
pub use support::SupportAgent;
pub use trait_::DynamicRoleTrait;
