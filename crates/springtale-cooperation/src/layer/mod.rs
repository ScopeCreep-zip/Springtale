//! Shared layer abstractions for the 7-tier routing architecture.
//!
//! See plan Phase K and COOPERATION.md §20 (revised) for the layer taxonomy.
//! Each concrete routing/coordination primitive lives in its own module
//! (stigmergy, routing, dissemination, contract_net, replan); this module
//! holds the identifiers and outcome types common to all of them.

pub mod trait_;
pub mod types;

pub use trait_::LayerAuthority;
pub use types::{LayerId, LayerOutcome, LayerResult};
