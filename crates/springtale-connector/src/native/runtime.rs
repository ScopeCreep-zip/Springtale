use async_trait::async_trait;

use crate::capability::grant::CapabilityChecker;
use crate::connector::trait_::{ActionResult, Connector, EventHandler};
use crate::error::ConnectorError;
use crate::host::ConnectorHost;
use crate::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

/// Host wrapper for a native (in-process) connector.
///
/// Wraps a `Box<dyn Connector>` with capability checking. The capability
/// check runs BEFORE every `execute()` — the inner connector cannot skip it.
///
/// Native connectors are first-party Rust crates. They run in-process
/// (no WASM sandbox) but are still subject to declared capability checks.
pub struct NativeConnectorHost {
    inner: Box<dyn Connector>,
    connector_name: String,
}

impl NativeConnectorHost {
    /// Create a new native connector host.
    ///
    /// Validates the manifest structure before accepting the connector.
    pub fn new(connector: Box<dyn Connector>) -> Result<Self, ConnectorError> {
        let manifest = connector.manifest();
        crate::manifest::verify::verify_manifest(manifest)?;

        let name = manifest.name.clone();

        Ok(Self {
            inner: connector,
            connector_name: name,
        })
    }

    /// Get the connector name.
    pub fn name(&self) -> &str {
        &self.connector_name
    }

    /// Execute an action with capability checking.
    ///
    /// The `checker` verifies the connector has the required capabilities
    /// before forwarding to the inner connector.
    pub async fn execute_checked(
        &self,
        action: &str,
        input: serde_json::Value,
        checker: &CapabilityChecker,
    ) -> Result<ActionResult, ConnectorError> {
        // Capability check BEFORE every execute — connector cannot skip it.
        // Uses per-action inference when possible, falls back to checking all
        // declared capabilities.
        super::capability::check_action_capabilities(
            checker,
            self.inner.manifest(),
            action,
            &input,
        )?;

        self.inner.execute(action, input).await
    }

    /// Delegate trigger registration to the inner connector.
    pub async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<crate::connector::subscription::Subscription, ConnectorError> {
        self.inner.on_event(trigger, handler).await
    }

    /// Delegate trigger removal to the inner connector.
    pub async fn remove_event(
        &self,
        sub: &crate::connector::subscription::Subscription,
    ) -> Result<(), ConnectorError> {
        self.inner.remove_event(sub).await
    }

    /// Get trigger declarations.
    pub fn triggers(&self) -> &[TriggerDecl] {
        self.inner.triggers()
    }

    /// Get action declarations.
    pub fn actions(&self) -> &[ActionDecl] {
        self.inner.actions()
    }

    /// Get the manifest.
    pub fn manifest(&self) -> &ConnectorManifest {
        self.inner.manifest()
    }

    /// Verify a webhook signature before dispatch.
    pub async fn verify_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ConnectorError> {
        self.inner.verify_webhook(headers, body).await
    }
}

#[async_trait]
impl ConnectorHost for NativeConnectorHost {
    fn name(&self) -> &str {
        &self.connector_name
    }

    async fn execute_checked(
        &self,
        action: &str,
        input: serde_json::Value,
        checker: &CapabilityChecker,
    ) -> Result<ActionResult, ConnectorError> {
        // Delegate to the existing method
        NativeConnectorHost::execute_checked(self, action, input, checker).await
    }

    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<crate::connector::subscription::Subscription, ConnectorError> {
        NativeConnectorHost::on_event(self, trigger, handler).await
    }

    async fn remove_event(
        &self,
        sub: &crate::connector::subscription::Subscription,
    ) -> Result<(), ConnectorError> {
        NativeConnectorHost::remove_event(self, sub).await
    }

    fn triggers(&self) -> &[TriggerDecl] {
        NativeConnectorHost::triggers(self)
    }

    fn actions(&self) -> &[ActionDecl] {
        NativeConnectorHost::actions(self)
    }

    fn manifest(&self) -> &ConnectorManifest {
        NativeConnectorHost::manifest(self)
    }

    async fn verify_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ConnectorError> {
        NativeConnectorHost::verify_webhook(self, headers, body).await
    }

    fn mention_extractor(&self) -> Option<&dyn crate::mention::MentionExtractor> {
        self.inner.mention_extractor()
    }

    fn normalize_event(&self, trigger: &str, raw: serde_json::Value) -> serde_json::Value {
        self.inner.normalize_event(trigger, raw)
    }
}
