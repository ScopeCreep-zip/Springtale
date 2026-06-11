//! Host functions registered into the Wasmtime linker for WASM guests.
//!
//! Each function gates through the capability checker before performing
//! the actual operation. WASM guests import these as `"springtale" "function_name"`.

use wasmtime::Linker;

use super::connector::HostState;
use crate::error::ConnectorError;

/// Register only `springtale.http_request`. Tier-aware linkers pull this
/// in for Warming/Hot/Fever; Cold omits it so Cold-tier guests trap at
/// instantiation when they import network I/O.
pub(crate) fn register_http_request(linker: &mut Linker<HostState>) -> Result<(), ConnectorError> {
    // Network outbound — gated by NetworkOutbound capability.
    // NOTE: When actual HTTP is added (Phase 2), wrap with
    // tokio::time::timeout() — epoch interrupts do NOT interrupt
    // host function calls (wasmtime#9188).
    // The guest calls this to request HTTP access. The host extracts the
    // URL from guest memory, checks the NetworkOutbound capability, and
    // returns 0 (allowed), -1 (invalid args), or -2 (capability denied).
    linker
        .func_wrap(
            "springtale",
            "http_request",
            |mut caller: wasmtime::Caller<'_, HostState>,
             url_ptr: i32,
             url_len: i32,
             _method_ptr: i32,
             _method_len: i32|
             -> i32 {
                // Extract URL from guest memory to check host.
                // Must get memory + read data before borrowing state.
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };
                let Ok(url_start) = usize::try_from(url_ptr) else {
                    return -1;
                };
                let Ok(url_length) = usize::try_from(url_len) else {
                    return -1;
                };
                let Some(url_end) = url_start.checked_add(url_length) else {
                    return -1;
                };
                let data = memory.data(&caller);
                if url_end > data.len() {
                    return -1;
                }
                let url_str = match std::str::from_utf8(&data[url_start..url_end]) {
                    Ok(s) => s.to_owned(),
                    Err(_) => return -1,
                };

                // Extract host from URL for capability check
                let host = match reqwest::Url::parse(&url_str) {
                    Ok(parsed) => parsed.host_str().unwrap_or("").to_owned(),
                    Err(_) => return -1,
                };

                // Gate: check NetworkOutbound capability
                let state = caller.data();
                if super::host_api::gate_network_outbound(
                    &state.checker,
                    &state.connector_name,
                    &host,
                )
                .is_err()
                {
                    return -2; // capability denied
                }

                0 // allowed
            },
        )
        .map_err(|e| ConnectorError::Sandbox(format!("failed to register http_request: {e}")))?;

    Ok(())
}
