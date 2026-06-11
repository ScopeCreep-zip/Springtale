#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the connector manifest TOML parser. The manifest schema declares
// capabilities and trigger/action surface; a parser crash is exploitable
// because connectors are loaded from disk before signature verification
// runs against the parsed schema.
//
// Wires into the public deserializer in springtale-connector once the
// crate exposes one. While the API is in flux we fuzz the raw TOML
// deserialization shape against a permissive Value type — same input
// surface, no panics tolerated.

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = toml::from_str::<toml::Value>(s);
    }
});
