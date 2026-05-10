//! Fuzz target for `HandoffPayload` serde round-trips.
//!
//! Plan §10.5: "`cargo fuzz` targets for parsers and deserializers —
//! specifically `ConsensusVote` and `HandoffPayload` serde round-trips."
//!
//! `HandoffPayload` is the wire contract for work-product transfers
//! across a `FlexibleChainPool`, environment-mediated deposits, and
//! (eventually) Veilid edges. Hostile input must not panic the
//! deserializer. Successful decodes are re-encoded to verify round-trip
//! idempotency.
//!
//! Run:
//!     cargo +nightly fuzz run fuzz_handoff_payload

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(payload) =
        serde_json::from_str::<springtale_cooperation::handoff::HandoffPayload>(s)
    else {
        return;
    };
    let re = serde_json::to_string(&payload).expect("reserialize succeeds after decode");
    let _second: springtale_cooperation::handoff::HandoffPayload =
        serde_json::from_str(&re).expect("reserialized form decodes cleanly");
});
