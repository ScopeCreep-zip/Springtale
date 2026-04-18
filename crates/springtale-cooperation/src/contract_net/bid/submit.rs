use crate::contract_net::channel::ParticipantHandle;
use crate::contract_net::types::{Bid, CnError};

/// Submit a bid back to the initiator over the mpsc channel. Split out from
/// `evaluate` so a simulator can inject bids without constructing channels
/// and a test can call `evaluate` without wiring the bus.
pub fn submit(handle: &ParticipantHandle, bid: Bid) -> Result<(), CnError> {
    handle
        .bid_tx
        .send(bid.clone())
        .map(|_| ())
        .map_err(|_| CnError::NotFound(bid.cfp_id))
}
