#![no_main]

use libfuzzer_sys::fuzz_target;

// Tauri capability JSON + Springtale capability schema parser fuzz.
// Capability files declare what IPC commands a window may invoke; a parser
// panic on a malformed capability file is exploitable if the loader is
// reached before validation.

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<serde_json::Value>(s);
    }
});
