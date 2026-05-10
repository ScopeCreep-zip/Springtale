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
//! ## Role catalog
//!
//! Three built-in Rust-impl roles live in this module:
//!
//! - `General` (`general.rs`): full access to the member's capabilities.
//! - `Information` (`information.rs`): read-only subset after primary
//!   loss — Siege dead→cameras.
//! - `Support` (`support.rs`): assist-only — Army of Two overwatch.
//!
//! ## Community roles
//!
//! Connectors — especially WASM community connectors — can't ship Rust
//! `impl DynamicRoleTrait` types directly. They contribute roles
//! *declaratively* via the manifest: name + capability list + action
//! allowlist (glob patterns). At install time, the runtime translates
//! each declaration into a `CommunityRole` factory in the shared
//! `RoleRegistry`. See `community.rs` and `registry.rs`.
//!
//! `RoleRegistry::build(name, capabilities)` is the single lookup path;
//! it replaces the legacy hand-rolled match in `from_name`.

pub mod apply;
pub mod community;
pub mod general;
pub mod information;
pub mod registry;
pub mod support;
pub mod trait_;

pub use apply::{
    apply_transformation, apply_transformation_via_registry, from_name,
    from_name_via_registry,
};
pub use community::CommunityRole;
pub use general::GeneralAgent;
pub use information::InformationAgent;
pub use registry::{RoleFactory, RoleRegistry};
pub use support::SupportAgent;
pub use trait_::DynamicRoleTrait;
