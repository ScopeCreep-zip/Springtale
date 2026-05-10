//! Dispatcher tasks — fan-out protocol messages to per-member inboxes and
//! drain intent-acks to the formation's cadence evaluator.

pub mod ack;
pub mod protocol;
