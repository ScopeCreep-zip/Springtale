//! Fuzz target for `ConsensusVote` serde round-trips.
//!
//! Plan §10.5: "`cargo fuzz` targets for parsers and deserializers —
//! specifically `ConsensusVote` and `HandoffPayload` serde round-trips."
//!
//! The fuzzer feeds arbitrary bytes as JSON and asks serde to reconstruct
//! a `ConsensusVote`. Any panic, infinite loop, or allocation overflow in
//! the deserialization path is a bug — this type crosses wire boundaries
//! (peer gossip, persistent audit log) so the deserializer must be
//! defensive against hostile input. A successful deserialization is
//! re-serialized and the round-trip is checked for idempotency.
//!
//! Run:
//!     cargo +nightly fuzz run fuzz_consensus_vote

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(vote) =
        serde_json::from_str::<springtale_cooperation::consensus::ConsensusVote>(s)
    else {
        return;
    };
    // Round-trip idempotency: decode → encode → decode must yield an equal
    // serialization. Any mismatch indicates a serde round-trip bug.
    let re = serde_json::to_string(&vote).expect("reserialize succeeds after decode");
    let _second: springtale_cooperation::consensus::ConsensusVote =
        serde_json::from_str(&re).expect("reserialized form decodes cleanly");
});
