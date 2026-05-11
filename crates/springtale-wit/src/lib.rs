//! WIT world definition for embedding Springtale cooperation primitives
//! in WASM Component Model hosts.
//!
//! The primary artifact of this crate is the `wit/cooperation.wit` file,
//! shipped alongside the crate so hosts can wire it into their own
//! `wit-bindgen` / `wasmtime` toolchains.
//!
//! Component Model hosts in scope (per `COOPERATION_IMPLEMENTATION_PLAN.md §15`):
//! - Bevy 0.14+ via wasmtime-as-runtime
//! - Unity via Wasmer (Component Model preview)
//! - wasmCloud (native Component Model host)
//! - Custom hosts targeting wasmtime ≥ 42.0.0
//!
//! Why a separate crate (not a feature on `springtale-cooperation`):
//! The `.wit` artifact ships independently. Hosts pulling
//! `springtale-wit` from crates.io get the WIT file via Cargo's `include`
//! shipping, and aren't forced to compile the whole cooperation crate.
//!
//! The crate's Rust surface is intentionally empty — there is no
//! Rust-side generated binding here. Hosts call `wit-bindgen` against
//! the shipped `.wit` directly in their own build script.

#![forbid(unsafe_code)]

/// Absolute path to the bundled `wit/cooperation.wit` file at compile
/// time. Useful when this crate is consumed as a build dependency and
/// the host's `build.rs` wants to pass the path to `wit-bindgen` /
/// `wasmtime::component::bindgen!`.
pub const COOPERATION_WIT_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/wit/cooperation.wit");

/// The literal source of the WIT world definition. Bundled so hosts
/// that prefer not to track filesystem paths can embed the WIT text
/// directly at build time.
pub const COOPERATION_WIT_SOURCE: &str = include_str!("../wit/cooperation.wit");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_source_is_non_empty() {
        assert!(COOPERATION_WIT_SOURCE.contains("package springtale:cooperation@0.1.0"));
        assert!(COOPERATION_WIT_SOURCE.contains("world springtale-cooperation"));
    }

    #[test]
    fn wit_path_resolves() {
        assert!(std::path::Path::new(COOPERATION_WIT_PATH).exists());
    }
}
