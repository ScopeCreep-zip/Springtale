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
use wasmtime::{Module, Store};

use super::limits::SandboxLimits;
use super::runtime::WasmEngine;
use super::tier::WasmTierCache;
use crate::capability::grant::CapabilityChecker;
use crate::connector::trait_::{ActionResult, EventHandler};
use crate::error::ConnectorError;
use crate::host::ConnectorHost;
use crate::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

/// Host state passed into the Wasmtime Store.
///
/// Accessible from host functions via `Caller::data()`. Contains the
/// capability checker and connector name for gating host calls.
///
/// Crate-visible so `wasm::tier::WasmTierCache` methods can name
/// `Store<HostState>` / `Linker<HostState>` in their crate-public
/// signatures without `private-interfaces` errors. Still not part of
/// the *external* connector API — outside callers go through
/// `WasmConnectorHost`.
pub(crate) struct HostState {
    pub(crate) connector_name: String,
    pub(crate) checker: CapabilityChecker,
    pub(crate) limits: wasmtime::StoreLimits,
}

/// WASM connector host — sandboxed execution of community connectors.
///
/// Each invocation creates a fresh `Store` with per-call resource limits
/// (fuel, memory, timeout). The compiled `Module` is shared across
/// invocations — compilation is expensive, execution is cheap.
///
/// Per COOPERATION.md §16, host-function availability is gated by the
/// formation's momentum tier. The tier is read per-invocation from the
/// `CapabilityChecker` (set by `CapabilityBridge` in springtale-runtime
/// based on the calling formation's momentum), NOT stored on the host
/// — that way the same WASM connector can be shared across formations
/// sitting at different tiers without races.
pub struct WasmConnectorHost {
    /// Shared Wasmtime engine (compiled once, reused).
    wasm_engine: Arc<WasmEngine>,
    /// Connector manifest (triggers, actions, capabilities).
    manifest: ConnectorManifest,
    /// Shared per-tier `InstancePre` cache. Instantiation at a tier is a
    /// hash lookup + `InstancePre::instantiate(store)` — no Linker rebuild.
    ///
    /// The compiled `Module` lives inside this cache as four `InstancePre`
    /// entries (one per tier), so we don't need to keep a separate
    /// `Module` field on the host.
    tier_cache: Arc<WasmTierCache>,
    /// Per-invocation sandbox limits.
    sandbox_limits: SandboxLimits,
}

impl WasmConnectorHost {
    /// Create a new WASM connector host from compiled module + manifest,
    /// registering the module against a shared tier cache.
    ///
    /// The module is validated and compiled at this point; the shared
    /// `WasmTierCache` pre-instantiates against each tier's Linker so
    /// subsequent `execute()` calls pay only `InstancePre::instantiate`.
    pub fn new(
        wasm_engine: Arc<WasmEngine>,
        wasm_bytes: &[u8],
        manifest: ConnectorManifest,
        sandbox_limits: SandboxLimits,
        tier_cache: Arc<WasmTierCache>,
    ) -> Result<Self, ConnectorError> {
        // Verify WASM hash against manifest if declared
        if let Some(ref expected_hash) = manifest.wasm_hash {
            WasmEngine::verify_wasm_hash(wasm_bytes, expected_hash)?;
        }

        // Compile the module (expensive — done once at install time)
        let module = Module::new(wasm_engine.engine(), wasm_bytes)
            .map_err(|e| ConnectorError::Sandbox(format!("module compilation failed: {e}")))?;

        // Register this module against every tier's Linker so momentum
        // transitions are a cheap `InstancePre::instantiate` away. The
        // cache keeps the Module internally — no need to retain it here.
        tier_cache.register_module(&manifest.name, &module)?;

        Ok(Self {
            wasm_engine,
            manifest,
            tier_cache,
            sandbox_limits,
        })
    }

    /// Module identity used as cache key.
    fn module_key(&self) -> &str {
        &self.manifest.name
    }

    /// Create a fresh store with per-invocation resource limits.
    fn create_store(&self, checker: &CapabilityChecker) -> Store<HostState> {
        let limits = self.wasm_engine.build_store_limits(&self.sandbox_limits);

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

        // Instantiate module at the tier carried by this invocation's
        // capability checker. Tier determines which host primitives were
        // linked (see `wasm::tier::primitives` + §16 of COOPERATION.md).
        let tier = checker.tier();
        let instance = self
            .tier_cache
            .instantiate_at_tier(self.module_key(), tier, &mut store)?;

        // Serialize input to JSON bytes for the guest
        let input_json = serde_json::to_string(&input)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;

        // Call the guest's exported action function
        // Convention: guest exports "execute" which takes (action_ptr, action_len, input_ptr, input_len)
        // and returns a pointer to the JSON result in guest memory.
        let execute_fn = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "execute")
            .map_err(|e| ConnectorError::Sandbox(format!("missing 'execute' export: {e}")))?;

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
            let pages_needed = ((total_needed - memory.data_size(&store)) / 65536) + 1;
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
                    i32::try_from(action_offset)
                        .map_err(|_| ConnectorError::Sandbox("action offset exceeds i32".into()))?,
                    i32::try_from(action_bytes.len())
                        .map_err(|_| ConnectorError::Sandbox("action length exceeds i32".into()))?,
                    i32::try_from(input_offset)
                        .map_err(|_| ConnectorError::Sandbox("input offset exceeds i32".into()))?,
                    i32::try_from(input_bytes.len())
                        .map_err(|_| ConnectorError::Sandbox("input length exceeds i32".into()))?,
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
        let rp = usize::try_from(result_ptr)
            .map_err(|_| ConnectorError::Sandbox("negative result pointer".into()))?;
        if rp + 4 > result_data.len() {
            return Err(ConnectorError::Sandbox(
                "result pointer out of bounds".into(),
            ));
        }
        let result_len = u32::from_le_bytes([
            result_data[rp],
            result_data[rp + 1],
            result_data[rp + 2],
            result_data[rp + 3],
        ]) as usize; // u32→usize: always safe (usize ≥ 32 bits)

        if result_len > self.sandbox_limits.max_response_bytes {
            return Err(ConnectorError::Sandbox(format!(
                "response too large: {} bytes (max {})",
                result_len, self.sandbox_limits.max_response_bytes
            )));
        }

        let result_start = rp + 4;
        let result_end = result_start + result_len;
        if result_end > result_data.len() {
            return Err(ConnectorError::Sandbox("result data out of bounds".into()));
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
        trigger: &str,
        _handler: EventHandler,
    ) -> Result<crate::connector::subscription::Subscription, ConnectorError> {
        // WASM connectors register triggers via manifest declarations.
        // The runtime matches incoming webhooks/events to the connector
        // and calls execute() with the trigger data as input.
        // Return a dummy subscription — WASM handlers are not used.
        Ok(crate::connector::subscription::Subscription {
            id: crate::connector::subscription::SubscriptionId(0),
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(
        &self,
        _sub: &crate::connector::subscription::Subscription,
    ) -> Result<(), ConnectorError> {
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
    use springtale_crypto::signature::SignatureAlgorithm;

    #[test]
    fn test_wasm_connector_host_rejects_hash_mismatch() {
        let engine = Arc::new(WasmEngine::new(SandboxLimits::default()).unwrap());
        let tier_cache = Arc::new(WasmTierCache::new(engine.clone()).unwrap());

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
            roles: vec![],
            wasm_hash: Some(
                "0000000000000000000000000000000000000000000000000000000000000000".into(),
            ),
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        };

        let result = WasmConnectorHost::new(
            engine,
            &wasm_bytes,
            manifest,
            SandboxLimits::default(),
            tier_cache,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_wasm_connector_host_creation_valid() {
        let engine = Arc::new(WasmEngine::new(SandboxLimits::default()).unwrap());
        let tier_cache = Arc::new(WasmTierCache::new(engine.clone()).unwrap());

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
            roles: vec![],
            wasm_hash: Some(hash),
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        };

        let result = WasmConnectorHost::new(
            engine,
            &wasm_bytes,
            manifest,
            SandboxLimits::default(),
            tier_cache,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_checker_tier_defaults_to_warming_and_roundtrips() {
        let checker = CapabilityChecker::new();
        // Warming is the permissive default — non-formation callers keep
        // HTTP capability (pre-Phase-16 behavior). Formations drop to Cold
        // via `with_tier` before execution.
        assert_eq!(checker.tier(), crate::tier::WasmTier::Warming);
        let hot = checker.clone().with_tier(crate::tier::WasmTier::Hot);
        assert_eq!(hot.tier(), crate::tier::WasmTier::Hot);
        // Original checker is unchanged — tier is per-clone, not shared.
        assert_eq!(checker.tier(), crate::tier::WasmTier::Warming);
        let cold = checker.with_tier(crate::tier::WasmTier::Cold);
        assert_eq!(cold.tier(), crate::tier::WasmTier::Cold);
    }
}
