//! WASM connector host — sandboxed execution via Wasmtime.
//!
//! Community connectors compile to `.wasm` and run in a Wasmtime sandbox
//! with fuel metering, memory limits, and wall-clock timeout. Host functions
//! are gated by the capability checker — a WASM guest calling network I/O
//! without `NetworkOutbound` in its manifest gets a trap, not a request.
//!
//! Per ARCHITECTURE.md: "Community connectors run in Wasmtime sandbox.
//! Fuel metering: 10M instructions per invocation. Memory limit: 64MB.
//! Wall-clock timeout: 30s."
//!
//! References:
//! - [Wasmtime security model](https://docs.wasmtime.dev/security.html)
//! - [WASM Component Model](https://tartanllama.xyz/posts/wasm-plugins/)

use std::sync::Arc;

use async_trait::async_trait;
use wasmtime::{Linker, Module, Store};

use super::limits::SandboxLimits;
use super::runtime::WasmEngine;
use crate::capability::grant::CapabilityChecker;
use crate::connector::trait_::{ActionResult, EventHandler};
use crate::error::ConnectorError;
use crate::host::ConnectorHost;
use crate::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

/// Host state passed into the Wasmtime Store.
///
/// Accessible from host functions via `Caller::data()`. Contains the
/// capability checker and connector name for gating host calls.
struct HostState {
    connector_name: String,
    checker: CapabilityChecker,
    limits: wasmtime::StoreLimits,
}

/// WASM connector host — sandboxed execution of community connectors.
///
/// Each invocation creates a fresh `Store` with per-call resource limits
/// (fuel, memory, timeout). The compiled `Module` is shared across
/// invocations — compilation is expensive, execution is cheap.
pub struct WasmConnectorHost {
    /// Shared Wasmtime engine (compiled once, reused).
    wasm_engine: Arc<WasmEngine>,
    /// Compiled WASM module (thread-safe, cloneable via Arc internally).
    module: Module,
    /// Connector manifest (triggers, actions, capabilities).
    manifest: ConnectorManifest,
    /// Pre-configured linker with host functions.
    linker: Arc<Linker<HostState>>,
    /// Per-invocation sandbox limits.
    sandbox_limits: SandboxLimits,
}

impl WasmConnectorHost {
    /// Create a new WASM connector host from compiled module + manifest.
    ///
    /// The module is validated and compiled at this point. Each `execute()`
    /// call creates a fresh `Store` with per-invocation limits.
    pub fn new(
        wasm_engine: Arc<WasmEngine>,
        wasm_bytes: &[u8],
        manifest: ConnectorManifest,
        sandbox_limits: SandboxLimits,
    ) -> Result<Self, ConnectorError> {
        // Verify WASM hash against manifest if declared
        if let Some(ref expected_hash) = manifest.wasm_hash {
            WasmEngine::verify_wasm_hash(wasm_bytes, expected_hash)?;
        }

        // Compile the module (expensive — done once at install time)
        let module = Module::new(wasm_engine.engine(), wasm_bytes)
            .map_err(|e| ConnectorError::Sandbox(format!("module compilation failed: {e}")))?;

        // Build linker with host functions
        let mut linker: Linker<HostState> = Linker::new(wasm_engine.engine());

        // Register host functions that WASM guests can call.
        // Each function gates through the capability checker before
        // performing the actual operation.
        Self::register_host_functions(&mut linker)?;

        Ok(Self {
            wasm_engine,
            module,
            manifest,
            linker: Arc::new(linker),
            sandbox_limits,
        })
    }

    /// Register host functions in the linker.
    ///
    /// WASM guests import these as `"springtale" "function_name"`.
    /// Each function checks capabilities before executing.
    fn register_host_functions(linker: &mut Linker<HostState>) -> Result<(), ConnectorError> {
        // Network outbound — gated by NetworkOutbound capability.
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
                    let url_start = url_ptr as usize;
                    let url_end = url_start + url_len as usize;
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

    /// Create a fresh store with per-invocation resource limits.
    fn create_store(&self, checker: &CapabilityChecker) -> Store<HostState> {
        let limits = self
            .wasm_engine
            .build_store_limits(&self.sandbox_limits);

        let host_state = HostState {
            connector_name: self.manifest.name.clone(),
            checker: checker.clone(),
            limits,
        };

        let mut store = Store::new(self.wasm_engine.engine(), host_state);

        // Apply resource limits
        store.limiter(|state| &mut state.limits);

        // Set fuel budget (instruction metering)
        if let Err(e) = store.set_fuel(self.sandbox_limits.fuel) {
            tracing::warn!(error = %e, "failed to set WASM fuel");
        }

        // Set epoch deadline for wall-clock timeout
        // The application layer must call engine.increment_epoch() periodically
        store.epoch_deadline_trap();
        store.set_epoch_deadline(self.sandbox_limits.timeout_secs);

        store
    }
}

#[async_trait]
impl ConnectorHost for WasmConnectorHost {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    async fn execute_checked(
        &self,
        action: &str,
        input: serde_json::Value,
        checker: &CapabilityChecker,
    ) -> Result<ActionResult, ConnectorError> {
        // Capability check BEFORE execution — same as native connectors
        crate::native::capability::check_action_capabilities(
            checker,
            &self.manifest,
            action,
            &input,
        )?;

        // Create a fresh store with per-invocation limits
        let mut store = self.create_store(checker);

        // Instantiate module in this store
        let instance = self
            .linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| ConnectorError::Sandbox(format!("instantiation failed: {e}")))?;

        // Serialize input to JSON bytes for the guest
        let input_json = serde_json::to_string(&input)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;

        // Call the guest's exported action function
        // Convention: guest exports "execute" which takes (action_ptr, action_len, input_ptr, input_len)
        // and returns a pointer to the JSON result in guest memory.
        let execute_fn = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "execute")
            .map_err(|e| {
                ConnectorError::Sandbox(format!("missing 'execute' export: {e}"))
            })?;

        // Write action name and input to guest memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| ConnectorError::Sandbox("guest has no 'memory' export".into()))?;

        let action_bytes = action.as_bytes();
        let input_bytes = input_json.as_bytes();

        // Allocate space in guest memory (after existing data)
        let action_offset = 1024; // convention: guest reserves first 1KB
        let input_offset = action_offset + action_bytes.len();

        // Bounds check
        let total_needed = input_offset + input_bytes.len();
        if total_needed > memory.data_size(&store) {
            // Grow memory if needed
            let pages_needed =
                ((total_needed - memory.data_size(&store)) / 65536) + 1;
            memory
                .grow(&mut store, pages_needed as u64)
                .map_err(|e| ConnectorError::Sandbox(format!("memory grow failed: {e}")))?;
        }

        memory.data_mut(&mut store)[action_offset..action_offset + action_bytes.len()]
            .copy_from_slice(action_bytes);
        memory.data_mut(&mut store)[input_offset..input_offset + input_bytes.len()]
            .copy_from_slice(input_bytes);

        // Call guest execute function
        let result_ptr = execute_fn
            .call(
                &mut store,
                (
                    action_offset as i32,
                    action_bytes.len() as i32,
                    input_offset as i32,
                    input_bytes.len() as i32,
                ),
            )
            .map_err(|e| {
                // Check if this was a fuel exhaustion or epoch timeout
                let msg = e.to_string();
                if msg.contains("fuel") {
                    ConnectorError::Sandbox(format!(
                        "connector exceeded instruction limit ({} fuel)",
                        self.sandbox_limits.fuel
                    ))
                } else if msg.contains("epoch") {
                    ConnectorError::Sandbox(format!(
                        "connector exceeded timeout ({}s)",
                        self.sandbox_limits.timeout_secs
                    ))
                } else {
                    ConnectorError::Sandbox(format!("execution failed: {e}"))
                }
            })?;

        // Read result from guest memory
        // Convention: result_ptr points to a JSON string in guest memory
        // terminated by a length returned in the first 4 bytes at result_ptr
        let result_data = memory.data(&store);
        let rp = result_ptr as usize;
        if rp + 4 > result_data.len() {
            return Err(ConnectorError::Sandbox(
                "result pointer out of bounds".into(),
            ));
        }
        let result_len =
            u32::from_le_bytes([result_data[rp], result_data[rp + 1], result_data[rp + 2], result_data[rp + 3]])
                as usize;

        if result_len > self.sandbox_limits.max_response_bytes {
            return Err(ConnectorError::Sandbox(format!(
                "response too large: {} bytes (max {})",
                result_len, self.sandbox_limits.max_response_bytes
            )));
        }

        let result_start = rp + 4;
        let result_end = result_start + result_len;
        if result_end > result_data.len() {
            return Err(ConnectorError::Sandbox(
                "result data out of bounds".into(),
            ));
        }

        let result_json = std::str::from_utf8(&result_data[result_start..result_end])
            .map_err(|e| ConnectorError::Sandbox(format!("invalid UTF-8 in result: {e}")))?;

        let result: ActionResult = serde_json::from_str(result_json)
            .map_err(|e| ConnectorError::Serialization(format!("invalid result JSON: {e}")))?;

        // Log fuel consumed for observability
        if let Ok(remaining) = store.get_fuel() {
            let consumed = self.sandbox_limits.fuel.saturating_sub(remaining);
            tracing::debug!(
                connector = %self.manifest.name,
                action = action,
                fuel_consumed = consumed,
                "WASM action executed"
            );
        }

        Ok(result)
    }

    async fn on_event(
        &self,
        _trigger: &str,
        _handler: EventHandler,
    ) -> Result<(), ConnectorError> {
        // WASM connectors register triggers via manifest declarations.
        // The runtime matches incoming webhooks/events to the connector
        // and calls execute() with the trigger data as input.
        Ok(())
    }

    fn triggers(&self) -> &[TriggerDecl] {
        &self.manifest.triggers
    }

    fn actions(&self) -> &[ActionDecl] {
        &self.manifest.actions
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn verify_webhook(
        &self,
        _headers: &std::collections::HashMap<String, String>,
        _body: &[u8],
    ) -> Result<(), ConnectorError> {
        // WASM connectors use HMAC verification via a host function.
        // The runtime handles webhook routing; the guest verifies
        // via the exported "verify_webhook" function if present.
        Err(ConnectorError::ExecutionFailed(
            "WASM webhook verification not supported — use manifest-declared webhook_secret".into(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_connector_host_rejects_hash_mismatch() {
        let engine = Arc::new(WasmEngine::new(SandboxLimits::default()).unwrap());

        // Minimal valid WASM module (empty module)
        let wasm_bytes = wat::parse_str("(module)").unwrap();

        let manifest = ConnectorManifest {
            name: "connector-test-wasm".into(),
            version: "0.1.0".into(),
            author: "test".into(),
            description: "test".into(),
            capabilities: vec![],
            triggers: vec![],
            actions: vec![],
            data_disclosure: vec![],
            wasm_hash: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
            signature: None,
        };

        let result = WasmConnectorHost::new(engine, &wasm_bytes, manifest, SandboxLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_wasm_connector_host_creation_valid() {
        let engine = Arc::new(WasmEngine::new(SandboxLimits::default()).unwrap());

        let wasm_bytes = wat::parse_str("(module (memory (export \"memory\") 1))").unwrap();

        // Compute correct hash
        use sha2::{Digest, Sha256};
        let hash = hex::encode(Sha256::digest(&wasm_bytes));

        let manifest = ConnectorManifest {
            name: "connector-test-wasm".into(),
            version: "0.1.0".into(),
            author: "test".into(),
            description: "test".into(),
            capabilities: vec![],
            triggers: vec![],
            actions: vec![],
            data_disclosure: vec![],
            wasm_hash: Some(hash),
            signature: None,
        };

        let result = WasmConnectorHost::new(engine, &wasm_bytes, manifest, SandboxLimits::default());
        assert!(result.is_ok());
    }
}
