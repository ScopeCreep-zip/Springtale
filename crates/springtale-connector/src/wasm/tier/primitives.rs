//! Tier-aware host function registration.
//!
//! Per COOPERATION.md §16 the momentum tier gates which host
//! primitives a WASM guest can call. Rather than checking the tier at
//! every function entry (expensive per-call), we build four Linkers
//! up front — one per tier — each registering only the host functions
//! allowed at that tier. The guest's import resolution at
//! `InstancePre::instantiate` time acts as the gate: if a host
//! function isn't in the Linker, the instantiate call fails.
//!
//! Current tier table (matches §7 capability table):
//!
//! | Host fn        | Cold | Warming | Hot | Fever |
//! |----------------|:----:|:-------:|:---:|:-----:|
//! | `http_request` |  —   |   ✓     |  ✓  |   ✓   |
//!
//! Network I/O is barred at Cold because the formation hasn't built
//! enough coherence to act responsibly outside its own workspace.
//! Future network/filesystem/shell host functions plug in here.

use wasmtime::Linker;

use super::super::connector::HostState;
use super::super::host_functions::register_http_request;
use crate::error::ConnectorError;
// Re-export so callers inside `wasm::tier` still see `WasmTier` in scope;
// the canonical definition lives at the connector-crate root in `tier.rs`
// so capability-side code can name it without requiring `wasm-sandbox`.
pub use crate::tier::WasmTier;

/// Build a `Linker` populated with exactly the host functions this tier
/// is allowed to call. Cold omits `http_request` entirely — guests
/// calling it trap at instantiation ("import not found").
///
/// Crate-visible only: callers outside the connector crate should use
/// `WasmTierCache` rather than building Linkers directly — `HostState`
/// is intentionally sealed within the wasm module.
pub(crate) fn register_tier_primitives(
    linker: &mut Linker<HostState>,
    tier: WasmTier,
) -> Result<(), ConnectorError> {
    match tier {
        WasmTier::Cold => {
            // No network, no filesystem, no shell. Cold agents may only
            // read the shared environment — which happens through the
            // cooperation layer, not through WASM host fns.
        }
        WasmTier::Warming | WasmTier::Hot | WasmTier::Fever => {
            register_http_request(linker)?;
        }
    }
    Ok(())
}
