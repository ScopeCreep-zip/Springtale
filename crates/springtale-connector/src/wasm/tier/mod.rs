//! Per-tier WASM instantiation cache (§16).
//!
//! `WasmTierCache` keeps four Linkers (one per momentum tier) and
//! pre-instantiates every registered module against each. Momentum
//! transitions then only pay `InstancePre::instantiate(store)` — no
//! compilation, no import resolution. See `cache.rs` for the full
//! rationale.

pub mod cache;
pub mod primitives;

pub use cache::WasmTierCache;
// `WasmTier` lives at the connector crate root (`crate::tier`) so the
// capability checker can name it without pulling in the `wasm-sandbox`
// feature. Re-exported here for ergonomic `wasm::tier::WasmTier` paths.
pub use crate::tier::WasmTier;
// `register_tier_primitives` is crate-visible only (leaks `HostState`),
// not re-exported here.
