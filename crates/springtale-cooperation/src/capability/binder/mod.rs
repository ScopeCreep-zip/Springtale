//! Momentum-tier → capability binder.
//!
//! Two-shape API:
//! - `unlocked_for_tier(tier)` — pure static projection (returns the
//!   cooperation-primitive names unlocked at `tier`). Used by
//!   `DynamicCapabilitySet::rebind_for_tier`.
//! - `Binder` trait + `DefaultBinder` — stateful projection that combines
//!   an agent's base capabilities with the tier-unlocked primitives.
//!   Consumers who need a single effective list per tick call
//!   `Binder::effective(&base, tier)`.

pub mod default;
pub mod static_table;
pub mod trait_;

pub use default::DefaultBinder;
pub use static_table::unlocked_for_tier;
pub use trait_::Binder;
