//! Handoff & transition — work product transfer between agents.
//!
//! Per COOPERATION.pdf §20: "Work products must pass between agents.
//! The handoff point is where most cooperative failures occur."

pub mod transfer;
mod types;

pub use transfer::{dispatch_handoff, HandoffResult};
pub use types::{HandoffPayload, HandoffType};
