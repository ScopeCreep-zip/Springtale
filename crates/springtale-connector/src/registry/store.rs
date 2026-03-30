use std::collections::HashMap;
use std::sync::Arc;

use crate::capability::grant::{CapabilityChecker, CapabilityPolicy};
use crate::connector::trait_::Connector;
use crate::error::ConnectorError;
use crate::native::runtime::NativeConnectorHost;

/// Entry in the connector registry.
///
/// The host is `Arc`-wrapped so the dispatch layer can clone the reference
/// and drop the registry lock before executing connector actions.
pub struct ConnectorEntry {
    /// The hosted connector (native or WASM).
    pub host: Arc<NativeConnectorHost>,
    /// Whether this connector is currently enabled.
    pub enabled: bool,
}

/// In-memory connector registry.
///
/// Manages installed connectors, their capabilities, and lifecycle.
/// Phase 1a: in-memory only. Persistence via springtale-store is wired
/// in the application layer (springtaled).
pub struct ConnectorRegistry {
    connectors: HashMap<String, ConnectorEntry>,
    capability_checker: CapabilityChecker,
    default_policy: CapabilityPolicy,
}

impl ConnectorRegistry {
    pub fn new(policy: CapabilityPolicy) -> Self {
        Self {
            connectors: HashMap::new(),
            capability_checker: CapabilityChecker::new(),
            default_policy: policy,
        }
    }

    /// Install a native connector.
    ///
    /// Delegates to `registry::loader::load_native()` for the verification
    /// pipeline (manifest validation, capability registration), then adds
    /// the connector to the registry.
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

        self.connectors.insert(
            name.clone(),
            ConnectorEntry {
                host: Arc::new(result.host),
                enabled: true,
            },
        );

        Ok(name)
    }

    /// Get a connector by name.
    pub fn get(&self, name: &str) -> Option<&ConnectorEntry> {
        self.connectors.get(name)
    }

    /// List all installed connectors.
    pub fn list(&self) -> Vec<(&str, bool)> {
        self.connectors
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.enabled))
            .collect()
    }

    /// Enable a connector.
    pub fn enable(&mut self, name: &str) -> Result<(), ConnectorError> {
        let entry = self
            .connectors
            .get_mut(name)
            .ok_or_else(|| ConnectorError::NotFound(name.to_owned()))?;
        entry.enabled = true;
        Ok(())
    }

    /// Disable a connector.
    pub fn disable(&mut self, name: &str) -> Result<(), ConnectorError> {
        let entry = self
            .connectors
            .get_mut(name)
            .ok_or_else(|| ConnectorError::NotFound(name.to_owned()))?;
        entry.enabled = false;
        Ok(())
    }

    /// Remove a connector from the registry.
    pub fn remove(&mut self, name: &str) -> Result<(), ConnectorError> {
        self.connectors
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| ConnectorError::NotFound(name.to_owned()))
    }

    /// Get a reference to the capability checker.
    pub fn capability_checker(&self) -> &CapabilityChecker {
        &self.capability_checker
    }

    /// Get a mutable reference to the capability checker (for approving pending caps).
    pub fn capability_checker_mut(&mut self) -> &mut CapabilityChecker {
        &mut self.capability_checker
    }

    /// Get a connector host and capability checker for out-of-lock execution.
    ///
    /// Returns an Arc-cloned host and cloned capability checker. The caller
    /// can drop the registry lock and then call `host.execute_checked()`
    /// without holding any lock across the network call.
    pub fn get_for_execute(
        &self,
        connector_name: &str,
    ) -> Result<(Arc<NativeConnectorHost>, CapabilityChecker), ConnectorError> {
        let entry = self
            .connectors
            .get(connector_name)
            .ok_or_else(|| ConnectorError::NotFound(connector_name.to_owned()))?;

        if !entry.enabled {
            return Err(ConnectorError::ExecutionFailed(format!(
                "connector '{connector_name}' is disabled"
            )));
        }

        Ok((Arc::clone(&entry.host), self.capability_checker.clone()))
    }

    /// Execute an action on a connector with capability checking.
    pub async fn execute(
        &self,
        connector_name: &str,
        action: &str,
        input: serde_json::Value,
    ) -> Result<crate::connector::trait_::ActionResult, ConnectorError> {
        let entry = self
            .connectors
            .get(connector_name)
            .ok_or_else(|| ConnectorError::NotFound(connector_name.to_owned()))?;

        if !entry.enabled {
            return Err(ConnectorError::ExecutionFailed(format!(
                "connector '{connector_name}' is disabled"
            )));
        }

        entry
            .host
            .execute_checked(action, input, &self.capability_checker)
            .await
    }
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new(CapabilityPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::trait_::{ActionResult, Connector, EventHandler};
    use crate::manifest::types::{ActionDecl, Capability, ConnectorManifest, TriggerDecl};
    use async_trait::async_trait;

    /// A minimal test connector for registry tests.
    struct TestConnector {
        manifest: ConnectorManifest,
    }

    impl TestConnector {
        fn new(name: &str) -> Self {
            Self {
                manifest: ConnectorManifest {
                    name: name.to_owned(),
                    version: "1.0.0".into(),
                    author: "test".into(),
                    description: "test connector".into(),
                    capabilities: vec![Capability::NetworkOutbound {
                        host: "api.example.com".into(),
                    }],
                    triggers: vec![TriggerDecl {
                        name: "test_event".into(),
                        description: "A test event".into(),
                        schema: None,
                    }],
                    actions: vec![ActionDecl {
                        name: "test_action".into(),
                        description: "A test action".into(),
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
            action: &str,
            _input: serde_json::Value,
        ) -> Result<ActionResult, ConnectorError> {
            Ok(ActionResult {
                success: true,
                output: serde_json::json!({"action": action}),
                message: "executed".into(),
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
    fn test_install_and_list() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        let name = registry
            .install_native(Box::new(TestConnector::new("connector-test")))
            .unwrap();
        assert_eq!(name, "connector-test");

        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "connector-test");
        assert!(list[0].1); // enabled
    }

    #[test]
    fn test_enable_disable() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(TestConnector::new("connector-test")))
            .unwrap();

        registry.disable("connector-test").unwrap();
        assert!(!registry.get("connector-test").unwrap().enabled);

        registry.enable("connector-test").unwrap();
        assert!(registry.get("connector-test").unwrap().enabled);
    }

    #[test]
    fn test_remove() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(TestConnector::new("connector-test")))
            .unwrap();
        assert_eq!(registry.list().len(), 1);

        registry.remove("connector-test").unwrap();
        assert!(registry.list().is_empty());
    }

    #[tokio::test]
    async fn test_execute_checked() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(TestConnector::new("connector-test")))
            .unwrap();

        let result = registry
            .execute("connector-test", "test_action", serde_json::json!({}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output["action"], "test_action");
    }

    #[tokio::test]
    async fn test_execute_disabled_connector_fails() {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(TestConnector::new("connector-test")))
            .unwrap();
        registry.disable("connector-test").unwrap();

        let result = registry
            .execute("connector-test", "test_action", serde_json::json!({}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_missing_connector_fails() {
        let registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        let result = registry
            .execute("nonexistent", "test_action", serde_json::json!({}))
            .await;

        assert!(result.is_err());
    }
}
