//! Handoff & transition — work product transfer between agents.
//!
//! Per COOPERATION.pdf §20: "Work products must pass between agents.
//! The handoff point is where most cooperative failures occur."

pub mod deposit;
pub mod flex_chain;
pub mod transfer;
mod types;

pub use flex_chain::FlexibleChainPool;
pub use transfer::{HandoffResult, dispatch_handoff, dispatch_handoff_durable};
pub use types::{HandoffPayload, HandoffType};
