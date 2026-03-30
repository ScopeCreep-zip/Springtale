use wasmtime::{Config, Engine, StoreLimits, StoreLimitsBuilder};

use super::limits::SandboxLimits;
use crate::error::ConnectorError;

/// Shared WASM engine — compiled once, reused across all WASM connectors.
///
/// The engine holds the compiler configuration. Each connector invocation
/// gets its own `Store` with per-invocation resource limits.
pub struct WasmEngine {
    engine: Engine,
    default_limits: SandboxLimits,
}

impl WasmEngine {
    /// Create a new WASM engine with the given default sandbox limits.
    ///
    /// The engine is configured with:
    /// - Fuel consumption enabled (instruction metering)
    /// - Epoch interruption enabled (wall-clock timeout)
    /// - Cranelift backend (optimizing compiler)
    /// - Component model enabled (WASI P2 support)
    pub fn new(default_limits: SandboxLimits) -> Result<Self, ConnectorError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);

        let engine = Engine::new(&config)
            .map_err(|e| ConnectorError::Sandbox(format!("engine creation failed: {e}")))?;

        Ok(Self {
            engine,
            default_limits,
        })
    }

    /// Get a reference to the inner wasmtime Engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get the default sandbox limits.
    pub fn default_limits(&self) -> &SandboxLimits {
        &self.default_limits
    }

    /// Build the `StoreLimits` for a connector invocation.
    pub fn build_store_limits(&self, limits: &SandboxLimits) -> StoreLimits {
        StoreLimitsBuilder::new()
            .memory_size(limits.memory_bytes)
            .instances(10) // max component instances
            .tables(10) // max tables
            .memories(2) // max linear memories
            .build()
    }

    /// Verify that a WASM binary's SHA-256 hash matches the declared hash.
    pub fn verify_wasm_hash(wasm_bytes: &[u8], expected_hex: &str) -> Result<(), ConnectorError> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        let actual = hex::encode(hasher.finalize());

        if actual != expected_hex {
            return Err(ConnectorError::WasmHashMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = WasmEngine::new(SandboxLimits::default());
        assert!(engine.is_ok());
    }

    #[test]
    fn test_store_limits() {
        let engine = WasmEngine::new(SandboxLimits::default()).unwrap();
        let limits = engine.build_store_limits(&SandboxLimits::default());
        // StoreLimits is opaque — we just verify it builds without error
        drop(limits);
    }

    #[test]
    fn test_verify_wasm_hash_valid() {
        use sha2::{Digest, Sha256};
        let data = b"fake wasm binary";
        let mut hasher = Sha256::new();
        hasher.update(data);
        let expected = hex::encode(hasher.finalize());

        let result = WasmEngine::verify_wasm_hash(data, &expected);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_wasm_hash_mismatch() {
        let result = WasmEngine::verify_wasm_hash(b"data", "0000000000000000");
        assert!(result.is_err());
    }
}
