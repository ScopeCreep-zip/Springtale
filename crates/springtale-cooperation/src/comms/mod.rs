//! Communication protocols — multi-layer inter-agent messaging.
//!
//! Per COOPERATION.pdf §19: agents need multiple simultaneous
//! communication layers, not a single channel.

pub mod bus;
pub mod channel;
mod types;

pub use bus::FormationBus;
pub use types::{
    BroadcastTrigger, CommChannel, MessageTarget, ProtocolPayload, StateMessage,
};
