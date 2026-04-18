//! Dynamic capability binding — agent capability sets change per momentum
//! tier, formation context, and role transformation.
//!
//! Per COOPERATION.md §16 (It Takes Two chapter-based reassignment): the
//! cooperative *structure* persists while the *tools* change. Here that
//! translates to a `DynamicCapabilitySet` with four layers (base, context,
//! momentum, transformed) and a `Binder` that projects them to a flat
//! effective set given a momentum tier.
//!
//! File split:
//! - `types.rs` — the layered `DynamicCapabilitySet` and its manipulators
//! - `binder/` — tier-aware projection logic (trait + default impl + tests)

pub mod binder;
pub mod decl;
pub mod types;

pub use binder::{Binder, DefaultBinder};
pub use decl::CapabilityDecl;
pub use types::DynamicCapabilitySet;
