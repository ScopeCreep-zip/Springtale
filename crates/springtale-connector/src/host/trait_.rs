//! ConnectorHost trait — execution-model-agnostic connector interface.
//!
//! The registry stores `Arc<dyn ConnectorHost>` so native connectors
//! (in-process Rust) and WASM connectors (sandboxed) share the same
//! dispatch path. Both perform capability checks before execution.

use async_trait::async_trait;

use crate::capability::grant::CapabilityChecker;
use crate::connector::subscription::Subscription;
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

    /// Register an event handler for a trigger. Returns a subscription handle.
    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<Subscription, ConnectorError>;

    /// Remove a previously registered event handler by subscription.
    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError>;

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

    /// The connector's chat ingestion half (see
    /// [`crate::chat::ChatSource`]), exposed through the host so the
    /// runtime can start and stop the loop without downcasting.
    /// Native hosts delegate to the inner connector; WASM hosts return
    /// `None` (a sandbox-side chat hook can follow, like
    /// `mention_extractor`).
    fn chat_source(&self) -> Option<crate::chat::SharedChatSource> {
        None
    }

    /// Per-connector mention extractor (D1) — exposed through the
    /// host so the universal harvester can call it without
    /// downcasting. Native hosts delegate to the inner connector's
    /// `mention_extractor()`. WASM hosts return `None` (WASM
    /// connectors run sandboxed and don't expose Rust trait
    /// objects — they would need a separate WASM-side hook,
    /// which Phase 2 of the sandbox effort can add).
    fn mention_extractor(&self) -> Option<&dyn crate::mention::MentionExtractor> {
        None
    }

    /// Normalize a raw provider event payload for `trigger` into the
    /// connector's declared trigger schema (the canonical shape recipes
    /// consume). The anti-corruption boundary — see
    /// [`crate::connector::trait_::Connector::normalize_event`]. Native
    /// hosts delegate to the inner connector; WASM hosts use identity
    /// (a sandbox-side hook can follow, like `mention_extractor`).
    fn normalize_event(&self, _trigger: &str, raw: serde_json::Value) -> serde_json::Value {
        raw
    }
}
