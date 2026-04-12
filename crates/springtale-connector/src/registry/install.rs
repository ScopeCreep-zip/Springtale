use std::sync::Arc;

use crate::connector::trait_::Connector;
use crate::error::ConnectorError;
use crate::host::ConnectorHost;

use super::store::{ConnectorEntry, ConnectorRegistry};

impl ConnectorRegistry {
    /// Install a native connector.
    ///
    /// Delegates to `registry::loader::load_native()` for the verification
    /// pipeline (manifest validation, capability registration), then adds
    /// the connector to the registry as `Arc<dyn ConnectorHost>`.
    pub fn install_native(
        &mut self,
        connector: Box<dyn Connector>,
    ) -> Result<String, ConnectorError> {
        let result = super::loader::load_native(
            connector,
            &mut self.capability_checker,
            &self.default_policy,
        )?;

        let name = result.host.name().to_owned();
        let host: Arc<dyn ConnectorHost> = Arc::new(result.host);

        self.connectors.insert(
            name.clone(),
            ConnectorEntry {
                host,
                enabled: true,
            },
        );

        Ok(name)
    }

    /// Install a WASM connector from compiled bytes + manifest.
    ///
    /// The WASM binary is compiled, hash-verified against the manifest,
    /// and loaded into a sandboxed host. Capabilities are registered
    /// from the manifest's declarations.
    #[cfg(feature = "wasm-sandbox")]
    pub fn install_wasm(
        &mut self,
        wasm_engine: std::sync::Arc<crate::wasm::WasmEngine>,
        wasm_bytes: &[u8],
        manifest: crate::manifest::types::ConnectorManifest,
        sandbox_limits: crate::wasm::SandboxLimits,
    ) -> Result<String, ConnectorError> {
        // Register capabilities from manifest
        self.capability_checker.register(
            &manifest.name,
            &manifest.capabilities,
            &self.default_policy,
        )?;

        // Create sandboxed WASM host
        let host = crate::wasm::WasmConnectorHost::new(
            wasm_engine,
            wasm_bytes,
            manifest.clone(),
            sandbox_limits,
        )?;

        let name = manifest.name.clone();
        let host: std::sync::Arc<dyn ConnectorHost> = std::sync::Arc::new(host);

        self.connectors.insert(
            name.clone(),
            ConnectorEntry {
                host,
                enabled: true,
            },
        );

        tracing::info!(connector = %name, "WASM connector installed (sandboxed)");
        Ok(name)
    }
}
