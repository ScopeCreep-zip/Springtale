use async_trait::async_trait;
use specta::Type;

use crate::connector::subscription::Subscription;
use crate::error::ConnectorError;
use crate::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

/// Result of executing a connector action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct ActionResult {
    /// Whether the action succeeded.
    pub success: bool,

    /// Output data from the action (connector-specific).
    pub output: serde_json::Value,

    /// Human-readable message (for logging/display).
    #[serde(default)]
    pub message: String,
}

/// Callback for connector events.
///
/// Connectors call this when a trigger fires. The handler is provided by
/// the application layer (springtaled or springtale-bot) and routes the
/// event into the rule engine.
pub type EventHandler = Box<dyn Fn(serde_json::Value) + Send + Sync>;

/// The Connector trait — the interface every connector implements.
///
/// Both `NativeConnector` and `WasmConnector` implement this trait.
/// The capability check layer wraps this trait and runs `check_capability()`
/// BEFORE every `execute()` call — connectors cannot skip it.
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    /// What events this connector can emit.
    fn triggers(&self) -> &[TriggerDecl];

    /// What actions this connector can perform.
    fn actions(&self) -> &[ActionDecl];

    /// Execute an action by name with the given input parameters.
    ///
    /// The capability layer has ALREADY verified that the caller has
    /// permission to invoke this action. The connector does not need
    /// to check capabilities internally.
    async fn execute(
        &self,
        action: &str,
        input: serde_json::Value,
    ) -> Result<ActionResult, ConnectorError>;

    /// Register a handler for a trigger event.
    ///
    /// Returns a `Subscription` handle. Store it per-rule and pass to
    /// `remove_event()` when the rule is disabled, deleted, or updated.
    ///
    /// Pattern: Home Assistant's attach_trigger → detach callback.
    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<Subscription, ConnectorError>;

    /// Remove a previously registered event handler.
    ///
    /// Called when a rule is disabled, deleted, or updated. The subscription
    /// ID locates the handler in the connector's internal list.
    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError>;

    /// Get the connector's manifest.
    fn manifest(&self) -> &ConnectorManifest;

    /// Verify an incoming webhook payload's signature.
    ///
    /// Called by the management API BEFORE dispatching a webhook event.
    /// Connectors with webhooks (GitHub, Kick, Telegram) override this
    /// to verify HMAC/RSA/Ed25519 signatures. Connectors without
    /// webhooks (Discord gateway, Slack Socket Mode, Nostr relay) can
    /// use the default implementation which rejects all webhooks.
    ///
    /// # Arguments
    /// * `headers` — HTTP headers from the webhook request
    /// * `body` — raw request body bytes (for HMAC computation)
    ///
    /// Default: returns `Err` (reject all webhooks for safety).
    async fn verify_webhook(
        &self,
        _headers: &std::collections::HashMap<String, String>,
        _body: &[u8],
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::ExecutionFailed(
            "this connector does not support webhooks".to_owned(),
        ))
    }

    /// Read an already-VERIFIED webhook payload: what chat messages and
    /// rule-engine events does it mean?
    ///
    /// Called by the management API's webhook ingress immediately after
    /// [`Connector::verify_webhook`] returns `Ok`. The ingress owns the
    /// transport (route, signature check, event log, rate limits); the
    /// connector owns the protocol — which field is the sender, which is
    /// the channel, which updates are chat at all.
    ///
    /// Before this existed the daemon extracted those fields itself with
    /// a `match` on one connector name, so only that connector's webhook
    /// chat could reach the bot and every other connector's webhook was
    /// receive-only.
    ///
    /// # Arguments
    /// * `trigger` — trigger name from the webhook route
    /// * `headers` — HTTP headers from the request (already verified)
    /// * `payload` — parsed JSON body (already verified, depth-checked)
    ///
    /// The ingress dispatches the route's own `ConnectorEvent` itself, so
    /// return an event in [`WebhookIngest::events`] only for an
    /// *additional* rule event the same request implies.
    ///
    /// Default: [`WebhookIngest::empty`] — correct for connectors with no
    /// webhooks and for gateway/polling connectors.
    async fn ingest_webhook(
        &self,
        _trigger: &str,
        _headers: &std::collections::HashMap<String, String>,
        _payload: &serde_json::Value,
    ) -> crate::webhook::WebhookIngest {
        crate::webhook::WebhookIngest::empty()
    }

    /// Normalize a raw provider event payload for `trigger` into this
    /// connector's declared trigger schema — the canonical flat shape
    /// recipes consume via `${trigger.*}`.
    ///
    /// This is the anti-corruption boundary (per the canonical-event /
    /// ATP webhook pattern): each provider's idiosyncratic raw payload
    /// (GitHub's nested webhook JSON, a raw Telegram `Update`, …) is
    /// mapped ONCE, here, to the fields declared in
    /// [`Connector::triggers`]. EVERY place a provider event becomes a
    /// rule-engine `TriggerEvent` — the webhook ingress and the polling
    /// gateways — calls this first, so recipes only ever see the
    /// connector's declared schema and never a raw nested blob.
    ///
    /// Default: identity. Correct for connectors whose emitted events
    /// already match their declared schema, and for generic passthrough
    /// (e.g. an arbitrary webhook body the recipe consumes whole).
    fn normalize_event(&self, _trigger: &str, raw: serde_json::Value) -> serde_json::Value {
        raw
    }

    /// Per-connector mention extractor (D1). Connectors that emit
    /// chat-like events (Telegram / Discord / Slack / Signal / IRC
    /// / Nostr) override this to teach the universal harvester how
    /// to find workspace keys in their event payloads. The
    /// harvester upserts each returned destination into the firing
    /// agent's formation mental_model_workspaces directory.
    ///
    /// Default returns `None` — connectors without chat-like
    /// events (Cron, Filesystem, HTTP, Browser, Shell) skip the
    /// harvest cleanly. See
    /// `springtale-connector::mention::MentionExtractor`.
    /// The connector's chat ingestion half, when it has one.
    ///
    /// Returning `Some` opts the connector into the runtime's chat
    /// wiring: `wire_chat` starts [`ChatSource::run`] when the
    /// connector is installed or enabled and stops it on disable or
    /// removal. Connectors with no chat surface leave this `None`.
    fn chat_source(&self) -> Option<crate::chat::SharedChatSource> {
        None
    }

    fn mention_extractor(&self) -> Option<&dyn crate::mention::MentionExtractor> {
        None
    }
}
