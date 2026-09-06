//! Default-deny WASI Preview 2 context for community connectors.
//!
//! Community connectors compile TS → JS → `jco componentize` to
//! `wasm32-wasip2`. A jco-built component imports `wasi:cli`, `wasi:io`,
//! `wasi:clocks`, `wasi:filesystem` and `wasi:random`, so with no WASI
//! context in the linker it cannot instantiate at all. This module
//! supplies one.
//!
//! The context grants **nothing**. Per the `wasmtime-wasi` docs a
//! freshly built [`WasiCtxBuilder`] means "stdin is closed, stdout and
//! stderr eat all input and it doesn't go anywhere, no env vars, no
//! arguments, and no preopens"; TCP/UDP exist as socket *types* but
//! every address is denied by default, and `wasi:sockets/ip-name-lookup`
//! is denied by default. That default is exactly the sandbox
//! SECURITY.md promises, so the builder is used bare.
//!
//! Deliberately never called here, and never to be added: `inherit_env`,
//! `inherit_stdio`, `inherit_stdin`, `inherit_stdout`, `inherit_stderr`,
//! `inherit_args`, `inherit_network`, `preopened_dir`, `allow_tcp`,
//! `allow_udp`, `allow_ip_name_lookup`, `socket_addr_check`.
//!
//! `wasi:sockets` is not linked at all. `add_to_linker_sync` would add
//! `tcp`, `udp`, `instance-network` and `ip-name-lookup` and leave the
//! boundary resting on an empty address set; omitting the interfaces
//! puts it on the absence of the import, so a connector reaching for a
//! socket fails to instantiate instead of failing at connect time. The
//! empty `WasiCtx` stays as the second line of defence. Network egress
//! and filesystem reads stay behind the `springtale.*` host functions in
//! `host_functions.rs`, which check the connector manifest's declared
//! capabilities with exact-host matching. Granting them through WASI
//! instead would hand the guest an ambient-authority path around the
//! capability checker.

use wasmtime::component::Linker as ComponentLinker;
use wasmtime_wasi::cli::{WasiCli, WasiCliView as _};
use wasmtime_wasi::filesystem::{WasiFilesystem, WasiFilesystemView as _};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

use super::connector::HostState;
use crate::error::ConnectorError;

/// Build the per-store WASI Preview 2 context with nothing granted.
///
/// One context per `Store` — WASI state is per-invocation, like fuel and
/// the epoch deadline.
pub(crate) fn default_deny_wasi_ctx() -> WasiCtx {
    // SECURITY: a bare builder is the deny-everything configuration —
    // no stdio, no env, no args, no preopens, no reachable socket
    // address, no DNS. No grant method is called, by design. Adding one
    // here widens the sandbox for every community connector at once.
    WasiCtxBuilder::new().build()
}

/// Add the synchronous WASI Preview 2 interfaces, minus `wasi:sockets`,
/// to a component linker.
///
/// The linker is a closed allow-list: an import the guest declares that
/// is neither a linked WASI interface nor a registered host function
/// fails at instantiation rather than trapping later.
pub(crate) fn add_wasi_to_linker(
    linker: &mut ComponentLinker<HostState>,
) -> Result<(), ConnectorError> {
    link_non_socket_interfaces(linker)
        .map_err(|e| ConnectorError::Sandbox(format!("add WASI p2 to component linker: {e}")))
}

/// Per-interface linking of the `wasi:cli/imports` world with every
/// `wasi:sockets` interface left out.
///
/// `wasmtime_wasi::p2::add_to_linker_sync` is deliberately not used: it
/// links `sockets/{tcp,udp,tcp-create-socket,udp-create-socket,
/// instance-network,ip-name-lookup}` unconditionally.
///
/// Not linked and not reachable from outside `wasmtime-wasi`:
/// `wasi:random/{insecure,insecure-seed}`, whose `add_to_linker` needs
/// the crate-private `WasiCtx::random` field. `wasi:random/random` — the
/// interface jco output actually imports — is linked below.
fn link_non_socket_interfaces(l: &mut ComponentLinker<HostState>) -> wasmtime::Result<()> {
    use wasmtime_wasi::p2::bindings::{cli, filesystem, sync};

    // The `wasi:http/proxy` import set: wasi:io (error, poll, streams),
    // wasi:clocks (wall-clock, monotonic-clock), wasi:random/random and
    // wasi:cli std{in,out,err}. It contains no sockets and no
    // filesystem, so it is the widest grant that can be taken wholesale.
    wasmtime_wasi::p2::add_to_linker_proxy_interfaces_sync(l)?;

    // The remainder of `wasi:cli/imports` that a jco-built component
    // pulls in, minus sockets.
    cli::exit::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;
    cli::environment::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;
    cli::terminal_input::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;
    cli::terminal_output::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;
    cli::terminal_stdin::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;
    cli::terminal_stdout::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;
    cli::terminal_stderr::add_to_linker::<HostState, WasiCli>(l, HostState::cli)?;

    // Filesystem interfaces link, but the context grants no preopens, so
    // the guest sees an empty preopen list and can open nothing. Real
    // file access stays behind the manifest-gated host functions.
    filesystem::preopens::add_to_linker::<HostState, WasiFilesystem>(l, HostState::filesystem)?;
    sync::filesystem::types::add_to_linker::<HostState, WasiFilesystem>(l, HostState::filesystem)?;
    Ok(())
}
