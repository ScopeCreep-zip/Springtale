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
    ///
    /// A name is registered once. Installing under a name that is already
    /// present returns [`ConnectorError::AlreadyRegistered`] before any
    /// state is touched — `load_native` would otherwise overwrite the
    /// existing entry's capability grant. Remove the connector first.
    pub fn install_native(
        &mut self,
        connector: Box<dyn Connector>,
    ) -> Result<String, ConnectorError> {
        let name = connector.manifest().name.clone();
        if self.connectors.contains_key(&name) {
            return Err(ConnectorError::AlreadyRegistered(name));
        }

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
    /// from the manifest's declarations. The shared `WasmTierCache`
    /// pre-instantiates the module at every momentum tier so subsequent
    /// tier flips are a cheap `InstancePre::instantiate` (see §16).
    ///
    /// A name is registered once, and first-party names are reserved:
    /// a manifest whose name matches an `inventory` factory entry is
    /// rejected with [`ConnectorError::NameReserved`] (even when that
    /// native connector is not currently installed), and a name already
    /// present in the registry is rejected with
    /// [`ConnectorError::AlreadyRegistered`]. Both checks run before the
    /// capability grant is registered so a rejected install leaves the
    /// existing connector's grant intact.
    #[cfg(feature = "wasm-sandbox")]
    pub fn install_wasm(
        &mut self,
        wasm_engine: std::sync::Arc<crate::wasm::WasmEngine>,
        wasm_bytes: &[u8],
        manifest: crate::manifest::types::ConnectorManifest,
        sandbox_limits: crate::wasm::SandboxLimits,
        tier_cache: std::sync::Arc<crate::wasm::WasmTierCache>,
    ) -> Result<String, ConnectorError> {
        let name = manifest.name.clone();
        if is_first_party_name(&name) {
            return Err(ConnectorError::NameReserved(name));
        }
        if self.connectors.contains_key(&name) {
            return Err(ConnectorError::AlreadyRegistered(name));
        }

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
            manifest,
            sandbox_limits,
            tier_cache,
        )?;

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

/// Whether `name` belongs to a first-party connector compiled into this
/// binary — an `inventory` [`crate::factory::FactoryEntry`]. Those names
/// are reserved: a community WASM manifest may not claim one, so a rule
/// that names `connector-github` can only ever dispatch into the native
/// first-party host.
#[cfg(feature = "wasm-sandbox")]
fn is_first_party_name(name: &str) -> bool {
    inventory::iter::<crate::factory::FactoryEntry>
        .into_iter()
        .any(|entry| entry.factory.name() == name)
}

#[cfg(all(test, feature = "wasm-sandbox"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use springtale_crypto::signature::SignatureAlgorithm;

    use crate::capability::grant::CapabilityPolicy;
    use crate::connector::trait_::Connector;
    use crate::error::ConnectorError;
    use crate::factory::{ConnectorFactory, FactoryEntry};
    use crate::manifest::types::ConnectorManifest;
    use crate::registry::store::ConnectorRegistry;
    use crate::registry::store::tests::TestConnector;
    use crate::wasm::{SandboxLimits, WasmEngine, WasmTierCache};

    /// Stand-in for the first-party `connector-github` factory. This
    /// crate's test binary links no connector crates, so the reserved-
    /// name check needs an `inventory` entry submitted here to match.
    struct ReservedNameFactory;

    #[async_trait::async_trait]
    impl ConnectorFactory for ReservedNameFactory {
        fn name(&self) -> &'static str {
            "connector-github"
        }
        fn config_key(&self) -> &'static str {
            "github"
        }
        async fn create(
            &self,
            _config: serde_json::Value,
        ) -> Result<Box<dyn Connector>, ConnectorError> {
            Ok(Box::new(TestConnector::new("connector-github")))
        }
    }

    inventory::submit!(FactoryEntry {
        factory: &ReservedNameFactory,
    });

    fn wasm_manifest(name: &str, wasm_bytes: &[u8]) -> ConnectorManifest {
        use sha2::{Digest, Sha256};
        ConnectorManifest {
            name: name.to_owned(),
            version: "0.1.0".into(),
            author: "test".into(),
            description: "test".into(),
            capabilities: vec![],
            triggers: vec![],
            actions: vec![],
            data_disclosure: vec![],
            roles: vec![],
            wasm_hash: Some(hex::encode(Sha256::digest(wasm_bytes))),
            signature_alg: SignatureAlgorithm::default(),
            signature: None,
        }
    }

    /// Install a minimal valid WASM module under `name`.
    fn install_wasm_named(
        registry: &mut ConnectorRegistry,
        name: &str,
    ) -> Result<String, ConnectorError> {
        let engine = Arc::new(WasmEngine::new(SandboxLimits::default()).unwrap());
        let tier_cache = Arc::new(WasmTierCache::new(engine.clone()).unwrap());
        let wasm_bytes = wat::parse_str("(module (memory (export \"memory\") 1))").unwrap();
        let manifest = wasm_manifest(name, &wasm_bytes);
        registry.install_wasm(
            engine,
            &wasm_bytes,
            manifest,
            SandboxLimits::default(),
            tier_cache,
        )
    }

    #[test]
    fn test_install_wasm_name_taken_by_native_returns_already_registered() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(TestConnector::new("connector-test")))
            .unwrap();

        let err = install_wasm_named(&mut registry, "connector-test").unwrap_err();

        assert!(
            matches!(err, ConnectorError::AlreadyRegistered(ref n) if n == "connector-test"),
            "unexpected error: {err}"
        );
        // The native entry is untouched.
        assert_eq!(registry.list().len(), 1);
        assert!(registry.get("connector-test").unwrap().enabled);
    }

    #[test]
    fn test_install_wasm_first_party_name_returns_name_reserved() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);

        let err = install_wasm_named(&mut registry, "connector-github").unwrap_err();

        assert!(
            matches!(err, ConnectorError::NameReserved(ref n) if n == "connector-github"),
            "unexpected error: {err}"
        );
        assert!(registry.list().is_empty());
    }
}
