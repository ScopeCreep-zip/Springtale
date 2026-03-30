use crate::capability::grant::{CapabilityChecker, CapabilityGrant, CapabilityPolicy};
use crate::connector::trait_::Connector;
use crate::error::ConnectorError;
use crate::native::runtime::NativeConnectorHost;

/// Result of loading a connector through the verification pipeline.
pub struct LoadResult {
    /// The hosted connector, ready for registration.
    pub host: NativeConnectorHost,
    /// The capability grant for this connector.
    pub grant: CapabilityGrant,
}

/// Load a native connector through the full verification pipeline.
///
/// Steps:
/// 1. Validate manifest structure (name, version, no wildcard hosts)
/// 2. Create NativeConnectorHost (wraps with capability checking)
/// 3. Register capabilities against the user's policy
///
/// Signature verification is separate — the caller verifies the manifest
/// signature BEFORE calling this function using `manifest::verify::verify_manifest_signature()`.
pub fn load_native(
    connector: Box<dyn Connector>,
    capability_checker: &mut CapabilityChecker,
    policy: &CapabilityPolicy,
) -> Result<LoadResult, ConnectorError> {
    // Step 1+2: validate manifest + create host
    let host = NativeConnectorHost::new(connector)?;
    let manifest = host.manifest();

    // Step 3: register capabilities
    let grant = capability_checker.register(&manifest.name, &manifest.capabilities, policy)?;

    if !grant.denied.is_empty() {
        tracing::warn!(
            connector = %manifest.name,
            denied = ?grant.denied,
            "some capabilities were denied"
        );
    }

    if !grant.pending_approval.is_empty() {
        tracing::info!(
            connector = %manifest.name,
            pending = ?grant.pending_approval,
            "some capabilities require user approval"
        );
    }

    tracing::info!(
        connector = %manifest.name,
        triggers = host.triggers().len(),
        actions = host.actions().len(),
        capabilities = manifest.capabilities.len(),
        approved = grant.approved.len(),
        "connector loaded through verification pipeline"
    );

    Ok(LoadResult { host, grant })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::trait_::{ActionResult, Connector, EventHandler};
    use crate::manifest::types::{ActionDecl, Capability, ConnectorManifest, TriggerDecl};
    use async_trait::async_trait;

    struct TestConnector {
        manifest: ConnectorManifest,
    }

    impl TestConnector {
        fn new() -> Self {
            Self {
                manifest: ConnectorManifest {
                    name: "connector-loader-test".into(),
                    version: "1.0.0".into(),
                    author: "test".into(),
                    description: "test".into(),
                    capabilities: vec![Capability::NetworkOutbound {
                        host: "api.example.com".into(),
                    }],
                    triggers: vec![],
                    actions: vec![ActionDecl {
                        name: "test".into(),
                        description: "test".into(),
                        input_schema: None,
                        output_schema: None,
                    }],
                    data_disclosure: vec![],
                    wasm_hash: None,
                    signature: None,
                },
            }
        }
    }

    #[async_trait]
    impl Connector for TestConnector {
        fn triggers(&self) -> &[TriggerDecl] {
            &self.manifest.triggers
        }
        fn actions(&self) -> &[ActionDecl] {
            &self.manifest.actions
        }
        async fn execute(
            &self,
            _action: &str,
            _input: serde_json::Value,
        ) -> Result<ActionResult, ConnectorError> {
            Ok(ActionResult {
                success: true,
                output: serde_json::json!({}),
                message: String::new(),
            })
        }
        async fn on_event(
            &self,
            _trigger: &str,
            _handler: EventHandler,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }
    }

    #[test]
    fn test_load_native_succeeds() {
        let mut checker = CapabilityChecker::new();
        let result = load_native(
            Box::new(TestConnector::new()),
            &mut checker,
            &CapabilityPolicy::AllowAll,
        );
        assert!(result.is_ok());
        let lr = result.unwrap();
        assert_eq!(lr.host.name(), "connector-loader-test");
        assert_eq!(lr.grant.approved.len(), 1);
    }

    #[test]
    fn test_load_native_with_denied_caps() {
        let mut checker = CapabilityChecker::new();
        let result = load_native(
            Box::new(TestConnector::new()),
            &mut checker,
            &CapabilityPolicy::DenyAll,
        );
        // Loading succeeds — denied caps are tracked but don't prevent loading
        assert!(result.is_ok());
        let lr = result.unwrap();
        assert!(lr.grant.approved.is_empty());
        assert_eq!(lr.grant.denied.len(), 1);
    }
}
