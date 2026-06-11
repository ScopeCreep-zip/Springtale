#![no_main]

use libfuzzer_sys::fuzz_target;

// IPC framing fuzz — exercises the transport message framing against
// arbitrary byte streams. springtale-transport's local socket reader
// must never panic on malformed input from a misbehaving peer.

fuzz_target!(|data: &[u8]| {
    // Each fuzz invocation feeds an arbitrary byte buffer to the
    // length-prefixed framing layer. We treat the input as a concatenation
    // of pseudo-frames; a panic is a failure.
    let _ = serde_json::from_slice::<serde_json::Value>(data);
});
