//! Communication protocols — multi-layer inter-agent messaging.
//!
//! Per COOPERATION.pdf §19: agents need multiple simultaneous
//! communication layers, not a single channel.

pub mod bus;
pub mod dispatcher;
mod types;

pub use bus::{
    AckDispatch, ChannelSendError, CohesionSignalMsg, DirectionalSignalMsg, FormationBus,
    FormationBusSubscription, IntentAckMsg, ProtocolDispatch, ProtocolMsg, StateBroadcastMsg,
};
pub use types::{
    BroadcastTrigger, CommChannel, MessageTarget, ProtocolPayload, StateMessage,
};
