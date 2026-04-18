//! L4 contested allocation — Contract Net Protocol (FIPA-CNP / JADE).
//!
//! Triggered when a task is high-value, rare-capability, or contested (e.g.
//! role-transformation candidate selection, sacrifice evaluation). Initiator
//! broadcasts a CFP, capable agents bid, initiator awards the winner. Uses
//! the `utility/` module for bid scoring so the scoring math stays
//! consistent with other decision surfaces.

pub mod award;
pub mod bid;
pub mod cfp;
pub mod channel;
pub mod coordinator;
pub mod lifecycle;
pub mod trait_;
pub mod types;

pub use channel::{CfpChannels, InitiatorHandle, ParticipantHandle};
pub use coordinator::{run_round, RoundOutcome};
pub use lifecycle::CfpState;
pub use trait_::{Bidder, Initiator};
pub use types::{Award, Bid, CallForProposals, CnError};
