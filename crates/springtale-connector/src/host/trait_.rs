//! ConnectorHost trait — execution-model-agnostic connector interface.
//!
//! The registry stores `Arc<dyn ConnectorHost>` so native connectors
//! (in-process Rust) and WASM connectors (sandboxed) share the same
//! dispatch path. Both perform capability checks before execution.

use async_trait::async_trait;

use crate::capability::grant::CapabilityChecker;
use crate::connector::trait_::{ActionResult, EventHandler};
use crate::error::ConnectorError;
use crate::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

/// Execution-model-agnostic connector host.
///
/// Native connectors (`NativeConnectorHost`) and WASM connectors
/// (`WasmConnectorHost`) both implement this trait. The registry
/// dispatches through `Arc<dyn ConnectorHost>` without knowing
/// which execution model is running underneath.
#[async_trait]
pub trait ConnectorHost: Send + Sync + 'static {
    /// Connector name (from manifest).
    fn name(&self) -> &str;

    /// Execute an action with capability checking.
    async fn execute_checked(
        &self,
        action: &str,
        input: serde_json::Value,
        checker: &CapabilityChecker,
    ) -> Result<ActionResult, ConnectorError>;

    /// Register an event handler for a trigger.
    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<(), ConnectorError>;

    /// Get trigger declarations.
    fn triggers(&self) -> &[TriggerDecl];

    /// Get action declarations.
    fn actions(&self) -> &[ActionDecl];

    /// Get the connector manifest.
    fn manifest(&self) -> &ConnectorManifest;

    /// Verify a webhook signature.
    async fn verify_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ConnectorError>;
}
