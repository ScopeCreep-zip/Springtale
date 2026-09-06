//! Rally & cascade recovery — formation self-healing before orchestrator escalation.
//!
//! Per COOPERATION.pdf §15:
//! Game sources: Total War general rally, routing cascade, Monster Hunter carts.

pub mod cascade;
mod types;

pub use types::{FormationRally, RallyEvent, RallyFailure, RallyResult, RallyTokens};
