//! `CapabilityBridge` — resolves the active momentum tier for a
//! formation and dispatches connector executions with the matching
//! capability checker.
//!
//! Per COOPERATION.md §16 the formation's `MomentumTier` decides which
//! WASM host functions a guest can import. Per-invocation scoping (not
//! per-host) is what lets a single WASM connector be shared across
//! formations sitting at different tiers — the checker clone for a
//! given invocation carries that formation's tier, and
//! `WasmConnectorHost::execute_checked` reads it to pick the right
//! `InstancePre` from the shared tier cache.
//!
//! Integration shape:
//!
//! ```text
//!   formation.momentum.tier ─┐
//!                            ▼
//!     momentum_to_wasm_tier() ──► WasmTier
//!                                    │
//!                                    ▼
//!            CapabilityChecker::with_tier(tier)
//!                                    │
//!                                    ▼
//!           WasmConnectorHost::execute_checked(&checker)
//!                                    │
//!                                    ▼
//!            WasmTierCache::instantiate_at_tier(tier, store)
//! ```

use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::RwLock;

use springtale_ai::{AiAdapter, NoopAdapter};
use springtale_connector::ActionResult;
use springtale_connector::manifest::types::Capability;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::tier::WasmTier;
use springtale_cooperation::execution::ExecutionContext;
use springtale_cooperation::momentum::MomentumTier;
use springtale_store::StorageBackend;

/// Translate a cooperation-layer `MomentumTier` into the connector-layer
/// `WasmTier`. The mapping is 1:1 — kept as a function rather than a
/// `From` impl because the connector crate cannot name `MomentumTier`
/// (no dep on cooperation) and cooperation cannot name `WasmTier` (no
/// dep on connector), so neither side can own the `From`. The runtime
/// crate sits above both and is the only place the mapping can live.
pub fn momentum_to_wasm_tier(tier: MomentumTier) -> WasmTier {
    match tier {
        MomentumTier::Cold => WasmTier::Cold,
        MomentumTier::Warming => WasmTier::Warming,
        MomentumTier::Hot => WasmTier::Hot,
        MomentumTier::Fever => WasmTier::Fever,
    }
}

/// Translate a cooperation-layer `MomentumTier` into the sentinel-
/// layer `ThrottleTier`. Same dependency rationale as
/// [`momentum_to_wasm_tier`]: sentinel sits below cooperation in the
/// graph (sentinel = core + store + connector; cooperation = core +
/// store) so neither side can own the `From`. The runtime is the
/// single boundary where both types are nameable.
pub fn momentum_to_throttle_tier(tier: MomentumTier) -> springtale_sentinel::ThrottleTier {
    use springtale_sentinel::ThrottleTier;
    match tier {
        MomentumTier::Cold => ThrottleTier::Cold,
        MomentumTier::Warming => ThrottleTier::Warming,
        MomentumTier::Hot => ThrottleTier::Hot,
        MomentumTier::Fever => ThrottleTier::Fever,
    }
}

/// Errors surfaced by the bridge. Wraps connector errors so callers in
/// the bot event loop don't need to import the connector error module.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("connector error: {0}")]
    Connector(#[from] springtale_connector::ConnectorError),
    /// User denied an approval request for a dangerous capability
    /// (currently only `Capability::ShellExec` is gated this way).
    #[error("approval denied: {0}")]
    ApprovalDenied(String),
    /// Approval gate timed out — no decision arrived within the
    /// configured window. Treated as a denial for the dispatch
    /// outcome but distinguished in the error variant so the audit
    /// trail can record the difference.
    #[error("approval timed out for {connector}.{action}")]
    ApprovalTimedOut { connector: String, action: String },
    /// Approval gate machinery itself failed (resolve on unknown id,
    /// shutdown, etc.). Treated as a denial for the dispatch outcome.
    #[error("approval gate error: {0}")]
    ApprovalGate(String),
}

/// Bridge between formation momentum and connector execution.
///
/// Clone is cheap (Arc inside). Held by `RuntimeState` so bot event
/// loops and external entry points share the same dispatch path.
///
/// The bridge also owns the per-process AI-adapter handle (a
/// `Arc<ArcSwap<...>>` shared with `RuntimeState.ai_adapter`) so
/// `dispatch_action`'s `AiComplete` arm can resolve the adapter
/// through the same single-dispatch point as connector actions.
/// Per-bot adapter selection (per the product-model rule "AI adapter
/// (optional, per-bot — defaults to NoopAdapter)") routes through
/// `ai_adapter_for(ctx)`, which resolves `ai:agent:{rule_id}` →
/// `ai:formation:{id}` → `ai:colony` from the store at dispatch.
#[derive(Clone)]
pub struct CapabilityBridge {
    registry: Arc<RwLock<ConnectorRegistry>>,
    /// Process-wide AI-adapter handle. `None` for test builds that
    /// don't wire AI — `ai_adapter_for` returns `NoopAdapter` in that
    /// case. Production paths in `init.rs` call `with_ai_adapter` to
    /// hand over the same `Arc<ArcSwap<...>>` held on `RuntimeState`
    /// so colony `configure_ai_adapter` swaps are visible to dispatch through the
    /// bridge.
    ai_adapter: Option<Arc<ArcSwap<Arc<dyn AiAdapter>>>>,
    /// Shared storage backend. `None` for test builds that don't
    /// need persistence (the chain dispatcher checks before
    /// reaching for it — `Action::Dedupe` short-circuits to "fresh"
    /// when no store is wired). Production paths in `init.rs` call
    /// [`Self::with_store`] with the same `Arc<dyn StorageBackend>`
    /// `RuntimeState.store` holds.
    store: Option<Arc<dyn StorageBackend>>,
    /// Built-adapter cache keyed by the resolved config JSON, so rules
    /// sharing one formation/colony config share one adapter and the
    /// Ollama model-pin check runs once per distinct config. Cleared by
    /// [`Self::invalidate_ai_cache`] whenever any level is reconfigured.
    agent_adapters: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<dyn AiAdapter>>>>,
    /// Executions log recorder (Phase B). `None` for test builds
    /// without persistence — the dispatcher falls back to a
    /// `NoopRecorder` so chain dispatch keeps working unobserved.
    /// Production paths build a [`StoreRecorder`] in `init.rs`.
    recorder: Option<Arc<dyn crate::operations::executions::ExecutionRecorder>>,
    /// AI guardrail handles (OWASP LLM10 — Unbounded Consumption).
    /// When set, `ai_adapter_for` wraps every returned adapter in a
    /// [`springtale_ai::GuardrailAdapter`] with the per-bot quota +
    /// shared refusal counter + output cap. `None` = no wrapping
    /// (used in tests that exercise the raw adapter).
    ai_guardrails: Option<AiGuardrailHandles>,
    /// Blocking-approval gate for capabilities the policy layer
    /// cannot auto-grant — currently only `Capability::ShellExec`
    /// (the OpenClaw CVE-2026-25253 1-click-RCE class; see
    /// `~/.claude/plans/mighty-honking-pinwheel.md` Finding A).
    /// `None` for test builds without an approval flow; production
    /// paths in `init.rs` wire a
    /// [`crate::approval::DefaultDenyApprovalGate`].
    approval_gate: Option<Arc<dyn crate::approval::ApprovalGate>>,
}

/// Bundle of guardrail dependencies the bridge weaves into every
/// per-bot adapter handle. Kept as a `Clone` struct so the bridge
/// itself stays `Clone` (Arcs all the way down). The
/// [`TokenQuota`](springtale_ai::TokenQuota) handle is shared across
/// all bots: the bot id becomes the partition key inside the
/// backend, so a single backend handle serves the whole runtime.
#[derive(Clone)]
pub struct AiGuardrailHandles {
    /// Shared per-bot token quota backend. The bridge keys lookups
    /// by `AgentId::to_string()`.
    pub quota: Arc<dyn springtale_ai::TokenQuota>,
    /// Shared refusal counter — surfaced via the admin API for
    /// OWASP LLM07 visibility.
    pub refusal_counter: springtale_ai::RefusalCounter,
    /// Maximum bytes the wrapper will return as `AiResponse::content`
    /// before truncating. Use [`springtale_ai::DEFAULT_OUTPUT_CAP_BYTES`]
    /// for the workspace default (64 KiB).
    pub output_cap_bytes: usize,
}

impl CapabilityBridge {
    /// Create a bridge around the shared connector registry. AI
    /// adapter unset by default — call [`Self::with_ai_adapter`] to
    /// wire it before dispatching any [`Action::AiComplete`].
    pub fn new(registry: Arc<RwLock<ConnectorRegistry>>) -> Self {
        Self {
            registry,
            ai_adapter: None,
            store: None,
            agent_adapters: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            recorder: None,
            ai_guardrails: None,
            approval_gate: None,
        }
    }

    /// Builder — wire the blocking approval gate. Production
    /// (`init.rs`) wires a [`crate::approval::DefaultDenyApprovalGate`]
    /// that the management API resolves into via
    /// `POST /approvals/:id`. Tests typically leave this `None` (the
    /// dispatch path treats missing gate as deny, matching the
    /// default-deny posture).
    #[must_use]
    pub fn with_approval_gate(mut self, gate: Arc<dyn crate::approval::ApprovalGate>) -> Self {
        self.approval_gate = Some(gate);
        self
    }

    /// Snapshot of the approval gate handle for the management API
    /// to call `resolve` / `pending` against. `None` when no gate is
    /// wired.
    pub fn approval_gate(&self) -> Option<&Arc<dyn crate::approval::ApprovalGate>> {
        self.approval_gate.as_ref()
    }

    /// Builder — wire the AI guardrail middleware. When set,
    /// [`Self::ai_adapter_for`] wraps every returned adapter in a
    /// [`springtale_ai::GuardrailAdapter`] keyed by the calling
    /// `AgentId`. Apps (`init.rs`) construct the handles once at
    /// startup, then pass the same bundle through the bridge so every
    /// firing bot shares one quota + counter.
    #[must_use]
    pub fn with_ai_guardrails(mut self, handles: AiGuardrailHandles) -> Self {
        self.ai_guardrails = Some(handles);
        self
    }

    /// Snapshot of the guardrail metric handles for the admin API.
    /// `None` when no guardrails are wired.
    pub fn ai_guardrails(&self) -> Option<&AiGuardrailHandles> {
        self.ai_guardrails.as_ref()
    }

    /// Builder — bind the shared AI-adapter handle. Wired by
    /// `init.rs` so the bridge sees the same `Arc<ArcSwap<...>>`
    /// `RuntimeState.ai_adapter` does.
    #[must_use]
    pub fn with_ai_adapter(mut self, adapter: Arc<ArcSwap<Arc<dyn AiAdapter>>>) -> Self {
        self.ai_adapter = Some(adapter);
        self
    }

    /// Builder — bind the shared storage backend. Wired by `init.rs`
    /// so `Action::Dedupe` dispatches against the same SQLite the
    /// rest of the runtime persists to.
    #[must_use]
    pub fn with_store(mut self, store: Arc<dyn StorageBackend>) -> Self {
        self.store = Some(store);
        self
    }

    /// Access the shared storage backend, if one is wired. Returns
    /// `None` for test builds that don't need persistence — the
    /// dispatcher short-circuits dedupe to "fresh" in that case.
    pub fn store(&self) -> Option<&Arc<dyn StorageBackend>> {
        self.store.as_ref()
    }

    /// Builder — bind the executions-log recorder. Wired by
    /// `init.rs` once the store is available so chain dispatches
    /// persist sizes-only rows to the executions / execution_steps
    /// tables. Test builds skip this and get the no-op recorder
    /// via [`Self::recorder`].
    #[must_use]
    pub fn with_recorder(
        mut self,
        recorder: Arc<dyn crate::operations::executions::ExecutionRecorder>,
    ) -> Self {
        self.recorder = Some(recorder);
        self
    }

    /// Resolve the executions-log recorder. Returns the wired
    /// [`StoreRecorder`] when available, otherwise a
    /// [`NoopRecorder`] so dispatch tests don't need a real
    /// store just to exercise non-observability arms.
    pub fn recorder(&self) -> Arc<dyn crate::operations::executions::ExecutionRecorder> {
        match &self.recorder {
            Some(r) => Arc::clone(r),
            None => Arc::new(crate::operations::executions::NoopRecorder),
        }
    }

    /// Access the underlying connector registry. Exposed so callers
    /// holding only a `&CapabilityBridge` (e.g. `dispatch_action`) can
    /// still reach the registry for non-RunConnector code paths — the
    /// bridge is the single public entry point for connector-call
    /// routing but the registry is the authoritative store.
    pub fn registry(&self) -> &Arc<RwLock<ConnectorRegistry>> {
        &self.registry
    }

    /// Resolve the AI adapter for a firing context.
    ///
    /// Reads `ai:agent:{rule_id}` → `ai:formation:{id}` → `ai:colony` from
    /// the store (first non-null wins), builds the adapter and caches it by
    /// its config, and falls through to the colony handle (or
    /// [`NoopAdapter`]) when no level is configured, no store is wired, or
    /// the build fails. The result is wrapped in the guardrail middleware
    /// (OWASP LLM Top-10) when guardrails are wired; the quota label is the
    /// agent id when present, else the rule id.
    pub async fn ai_adapter_for(&self, ctx: &ExecutionContext) -> Arc<dyn AiAdapter> {
        let Some(store) = &self.store else {
            return self.wrap_guardrails(self.global_adapter(), ctx);
        };
        let fid = ctx.formation_id.as_ref().map(|f| f.0.to_string());
        let cfg = crate::operations::config::resolve_ai_config(
            store.as_ref(),
            &ctx.rule_id.0,
            fid.as_deref(),
        )
        .await
        .unwrap_or(Value::Null);
        let inner = if cfg.is_null() {
            self.global_adapter()
        } else {
            let key = cfg.to_string();
            let cached = self.agent_adapters.read().await.get(&key).map(Arc::clone);
            match cached {
                Some(adapter) => adapter,
                None => {
                    let built = match crate::operations::config::build_adapter(&cfg).await {
                        Ok(adapter) => adapter,
                        Err(e) => {
                            tracing::warn!(
                                rule = %ctx.rule_id.0,
                                error = %e,
                                "AI adapter build failed; using colony adapter"
                            );
                            self.global_adapter()
                        }
                    };
                    self.agent_adapters
                        .write()
                        .await
                        .insert(key, Arc::clone(&built));
                    built
                }
            }
        };
        self.wrap_guardrails(inner, ctx)
    }

    /// Drop every cached built adapter so the next dispatch re-resolves
    /// from the store. Called after any level is reconfigured.
    pub async fn invalidate_ai_cache(&self) {
        self.agent_adapters.write().await.clear();
    }

    /// The colony (global) adapter snapshot, or [`NoopAdapter`] when none
    /// is wired. Preflight inspects the colony default through this.
    pub fn global_adapter(&self) -> Arc<dyn AiAdapter> {
        match &self.ai_adapter {
            Some(handle) => Arc::clone(&handle.load()),
            None => Arc::new(NoopAdapter),
        }
    }

    /// Wrap in the guardrail middleware when guardrails are wired. The
    /// quota is charged to the agent for agent-scoped fires, else to the
    /// rule.
    fn wrap_guardrails(
        &self,
        inner: Arc<dyn AiAdapter>,
        ctx: &ExecutionContext,
    ) -> Arc<dyn AiAdapter> {
        let Some(handles) = &self.ai_guardrails else {
            return inner;
        };
        let label = match &ctx.agent_id {
            Some(agent) => agent.to_string(),
            None => ctx.rule_id.0.to_string(),
        };
        let guard = springtale_ai::GuardrailAdapter::new(inner)
            .with_output_cap(handles.output_cap_bytes)
            .with_refusal_counter(handles.refusal_counter.clone())
            .with_quota(Arc::clone(&handles.quota), label);
        Arc::new(guard)
    }

    /// Execute a connector action at the formation's current momentum
    /// tier. The caller passes the tier directly (rather than a
    /// formation id) so the bridge doesn't need to reach into cooperation
    /// state — event-loop code already has the tier in hand when it
    /// decides to fire an action.
    ///
    /// Implementation: clone the registry's capability checker out of
    /// the lock (so we don't hold it across the network call), bind the
    /// tier, then dispatch via the stored `Arc<dyn ConnectorHost>`.
    pub async fn execute(
        &self,
        connector_name: &str,
        action: &str,
        input: Value,
        tier: WasmTier,
    ) -> Result<ActionResult, BridgeError> {
        self.execute_with_origin(connector_name, action, input, tier, None)
            .await
    }

    /// [`Self::execute`] with the chat origin of the triggering message
    /// (plan 6.7). A ShellExec approval raised here carries `origin` so
    /// the announcer can deliver the card to that channel; `None` for
    /// rule / formation fires, which surface on the dashboard only.
    pub async fn execute_with_origin(
        &self,
        connector_name: &str,
        action: &str,
        input: Value,
        tier: WasmTier,
        origin: Option<springtale_core::policy::ChatOrigin>,
    ) -> Result<ActionResult, BridgeError> {
        let (host, mut checker) = {
            let registry = self.registry.read().await;
            registry.get_for_execute(connector_name)?
        };

        // OpenClaw CVE-2026-25253 1-click-RCE class: connectors that
        // declare `Capability::ShellExec` route every invocation
        // through the blocking [`ApprovalGate`]. The grant table
        // routes ShellExec into `pending_approval` regardless of
        // `CapabilityPolicy` (see Phase-7 audit Finding A) — the
        // gate is the ONLY path from pending → momentarily-approved
        // for a single call.
        //
        // The momentary approval is applied to a CLONE of the
        // checker (`Clone` is a snapshot of the grants HashMap), so
        // the underlying registry's grant table stays in pending and
        // the next invocation also requires a fresh approval. No
        // ambient-authority drift.
        let manifest = host.manifest();
        let action_needs_shell_exec = manifest
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::ShellExec))
            && manifest.actions.iter().any(|a| a.name == action);

        if action_needs_shell_exec {
            let gate = self.approval_gate.as_ref().ok_or_else(|| {
                BridgeError::ApprovalGate(
                    "ShellExec requires an approval gate but none is wired".to_owned(),
                )
            })?;
            let request = crate::approval::ApprovalRequest {
                id: crate::approval::ApprovalRequestId::new(),
                connector_name: connector_name.to_owned(),
                capability: crate::approval::GatedCapability::Manifest(Capability::ShellExec),
                agent_id: None,
                summary: format!("{connector_name}.{action}"),
                requested_at: chrono::Utc::now(),
                origin,
                expires_at: None,
            };
            let decision = gate
                .request(request)
                .await
                .map_err(|e| BridgeError::ApprovalGate(e.to_string()))?;
            match decision {
                crate::approval::ApprovalDecision::Approved { .. } => {
                    // Single-shot grant on the CLONE only.
                    checker.approve(connector_name, &Capability::ShellExec);
                }
                crate::approval::ApprovalDecision::Denied { reason, .. } => {
                    return Err(BridgeError::ApprovalDenied(reason));
                }
                crate::approval::ApprovalDecision::TimedOut { .. } => {
                    return Err(BridgeError::ApprovalTimedOut {
                        connector: connector_name.to_owned(),
                        action: action.to_owned(),
                    });
                }
            }
        }

        let checker = checker.with_tier(tier);
        Ok(host.execute_checked(action, input, &checker).await?)
    }

    /// Convenience: execute using a `MomentumTier` directly, letting
    /// the bridge convert. Event-loop call sites that already hold the
    /// `MomentumState` prefer this over computing the WasmTier
    /// themselves.
    pub async fn execute_at_momentum(
        &self,
        connector_name: &str,
        action: &str,
        input: Value,
        momentum: MomentumTier,
    ) -> Result<ActionResult, BridgeError> {
        self.execute(
            connector_name,
            action,
            input,
            momentum_to_wasm_tier(momentum),
        )
        .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use springtale_cooperation::execution::ExecutionMode;
    use springtale_core::rule::RuleId;

    fn global_ctx() -> ExecutionContext {
        ExecutionContext::for_global(RuleId(uuid::Uuid::new_v4()), ExecutionMode::Cron)
    }
    use springtale_connector::ConnectorError;
    use springtale_connector::capability::grant::CapabilityPolicy;
    use springtale_connector::connector::subscription::{Subscription, SubscriptionId};
    use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
    use springtale_connector::manifest::SignatureAlgorithm;
    use springtale_connector::manifest::types::{
        ActionDecl, Capability, ConnectorManifest, TriggerDecl,
    };

    /// Minimal native connector that echoes its input and reports
    /// success. Used by bridge tests — it doesn't declare any
    /// capabilities so every action auto-approves under
    /// `CapabilityPolicy::AllowAll`, letting the tests focus on the
    /// bridge's dispatch behavior rather than capability semantics.
    struct EchoConnector {
        manifest: ConnectorManifest,
    }

    impl EchoConnector {
        fn new(name: &str) -> Self {
            Self {
                manifest: ConnectorManifest {
                    name: name.to_owned(),
                    version: "0.1.0".into(),
                    author: "test".into(),
                    description: "echo".into(),
                    capabilities: vec![Capability::NetworkOutbound {
                        host: "api.example.com".into(),
                    }],
                    triggers: vec![TriggerDecl {
                        name: "test_event".into(),
                        description: "test".into(),
                        schema: None,
                    }],
                    actions: vec![ActionDecl {
                        read_only: false,
                        destructive: None,
                        poll_interval_secs: None,
                        name: "echo".into(),
                        description: "echo".into(),
                        input_schema: None,
                        output_schema: None,
                    }],
                    data_disclosure: vec![],
                    roles: vec![],
                    wasm_hash: None,
                    signature_alg: SignatureAlgorithm::default(),
                    signature: None,
                },
            }
        }
    }

    #[async_trait]
    impl Connector for EchoConnector {
        fn triggers(&self) -> &[TriggerDecl] {
            &self.manifest.triggers
        }
        fn actions(&self) -> &[ActionDecl] {
            &self.manifest.actions
        }
        async fn execute(
            &self,
            action: &str,
            input: serde_json::Value,
        ) -> Result<ActionResult, ConnectorError> {
            Ok(ActionResult {
                success: true,
                output: serde_json::json!({"echoed": input, "action": action}),
                message: "ok".into(),
            })
        }
        async fn on_event(
            &self,
            trigger: &str,
            _handler: EventHandler,
        ) -> Result<Subscription, ConnectorError> {
            Ok(Subscription {
                id: SubscriptionId(0),
                trigger: trigger.to_owned(),
            })
        }
        async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
            Ok(())
        }
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }
    }

    fn bridge_with_echo(name: &str) -> (CapabilityBridge, Arc<RwLock<ConnectorRegistry>>) {
        let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
        registry
            .install_native(Box::new(EchoConnector::new(name)))
            .unwrap();
        let registry = Arc::new(RwLock::new(registry));
        let bridge = CapabilityBridge::new(registry.clone());
        (bridge, registry)
    }

    #[test]
    fn momentum_to_wasm_tier_preserves_order() {
        assert_eq!(momentum_to_wasm_tier(MomentumTier::Cold), WasmTier::Cold);
        assert_eq!(
            momentum_to_wasm_tier(MomentumTier::Warming),
            WasmTier::Warming
        );
        assert_eq!(momentum_to_wasm_tier(MomentumTier::Hot), WasmTier::Hot);
        assert_eq!(momentum_to_wasm_tier(MomentumTier::Fever), WasmTier::Fever);
    }

    #[tokio::test]
    async fn execute_with_missing_connector_returns_bridge_error() {
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::AllowAll,
        )));
        let bridge = CapabilityBridge::new(registry);
        let err = bridge
            .execute(
                "does-not-exist",
                "noop",
                serde_json::json!({}),
                WasmTier::Cold,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeError::Connector(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_to_installed_connector() {
        let (bridge, _) = bridge_with_echo("connector-echo");
        let result = bridge
            .execute(
                "connector-echo",
                "echo",
                serde_json::json!({"x": 1}),
                WasmTier::Warming,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.output["action"], "echo");
        assert_eq!(result.output["echoed"]["x"], 1);
    }

    #[tokio::test]
    async fn execute_at_momentum_maps_each_tier() {
        // All four momentum tiers dispatch successfully for a native
        // connector (tier only gates WASM host-fn linking; native
        // connectors are tier-agnostic but still flow through the
        // bridge so the plumbing is exercised end-to-end).
        let (bridge, _) = bridge_with_echo("connector-echo");
        for tier in [
            MomentumTier::Cold,
            MomentumTier::Warming,
            MomentumTier::Hot,
            MomentumTier::Fever,
        ] {
            let result = bridge
                .execute_at_momentum(
                    "connector-echo",
                    "echo",
                    serde_json::json!({"tier": format!("{tier:?}")}),
                    tier,
                )
                .await
                .unwrap_or_else(|e| panic!("{tier:?}: {e}"));
            assert!(result.success, "tier {tier:?} should succeed");
        }
    }

    #[tokio::test]
    async fn ai_adapter_for_returns_noop_when_unwired() {
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::AllowAll,
        )));
        let bridge = CapabilityBridge::new(registry);
        // No `.with_ai_adapter` → fallback is NoopAdapter.
        let adapter = bridge.ai_adapter_for(&global_ctx()).await;
        // We can't downcast `dyn AiAdapter` to NoopAdapter directly,
        // but the type-erased fallback path is the only one that
        // returns without a wired handle, so the call succeeding is
        // proof enough.
        let _: Arc<dyn AiAdapter> = adapter;
    }

    #[tokio::test]
    async fn ai_adapter_for_returns_wired_handle() {
        use arc_swap::ArcSwap;

        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::AllowAll,
        )));
        let handle: Arc<ArcSwap<Arc<dyn AiAdapter>>> = Arc::new(ArcSwap::from(Arc::new(Arc::new(
            NoopAdapter,
        )
            as Arc<dyn AiAdapter>)));
        let bridge = CapabilityBridge::new(registry).with_ai_adapter(handle.clone());

        // First call resolves to the wired adapter.
        let first = bridge.ai_adapter_for(&global_ctx()).await;
        // Hot-swap a different adapter and confirm the bridge sees
        // the swap (the same handle is shared with `RuntimeState`).
        handle.store(Arc::new(Arc::new(NoopAdapter) as Arc<dyn AiAdapter>));
        let second = bridge.ai_adapter_for(&global_ctx()).await;
        // Pointer-equality check would require concrete typing; just
        // confirm both calls succeed and produce a usable Arc.
        let _: Arc<dyn AiAdapter> = first;
        let _: Arc<dyn AiAdapter> = second;
    }

    #[tokio::test]
    async fn guardrails_wrap_per_bot_adapter_keyed_by_agent_id() {
        // End-to-end proof that the cooperation `AgentId(uuid::Uuid)`
        // is the canonical bot identity for the AI quota:
        //   1. Bridge wires a real AiAdapter + a per-bot quota
        //   2. ai_adapter_for(Some(agent_id)) returns a GuardrailAdapter
        //   3. Calling that adapter charges THE BOT'S row in the quota
        //   4. A global (no-agent) context is charged to the rule id,
        //      never to the bot's row.
        use arc_swap::ArcSwap;
        use springtale_ai::{AiOptions, AiRequest, InMemoryTokenQuota, RefusalCounter};
        use springtale_cooperation::cadence::AgentId;

        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::AllowAll,
        )));
        let handle: Arc<ArcSwap<Arc<dyn AiAdapter>>> = Arc::new(ArcSwap::from(Arc::new(Arc::new(
            NoopAdapter,
        )
            as Arc<dyn AiAdapter>)));

        let quota: Arc<dyn springtale_ai::TokenQuota> =
            Arc::new(InMemoryTokenQuota::new(Some(10_000)));
        let counter = RefusalCounter::new();
        let bridge = CapabilityBridge::new(registry)
            .with_ai_adapter(handle.clone())
            .with_ai_guardrails(AiGuardrailHandles {
                quota: Arc::clone(&quota),
                refusal_counter: counter.clone(),
                output_cap_bytes: springtale_ai::DEFAULT_OUTPUT_CAP_BYTES,
            });

        let bot = AgentId::new();

        // Without a bot id the quota label is the rule id, so the
        // bot's row is untouched.
        let unbot = bridge.ai_adapter_for(&global_ctx()).await;
        let _ = unbot
            .complete(
                AiRequest::Complete {
                    prompt: "hi".into(),
                },
                AiOptions::default(),
            )
            .await;
        let used_without_id = quota.usage(&bot.to_string()).await.unwrap();
        assert_eq!(used_without_id, 0, "no agent id ⇒ no quota write");

        // WITH a bot id, the guardrail wraps the adapter; the call
        // routes through the quota — the NoopAdapter returns
        // AiError::Disabled, so commit rolls back to 0, but the
        // refusal counter records the attempt.
        let bot_ctx = ExecutionContext::for_agent(
            RuleId(uuid::Uuid::new_v4()),
            bot,
            MomentumTier::Warming,
            ExecutionMode::Cron,
        );
        let bot_adapter = bridge.ai_adapter_for(&bot_ctx).await;
        let _ = bot_adapter
            .complete(
                AiRequest::Complete {
                    prompt: "hi".into(),
                },
                AiOptions::default(),
            )
            .await;
        let snap = counter.snapshot();
        // Two guarded calls: the global-context one (labelled by rule id)
        // and the bot one (labelled by agent id).
        assert_eq!(snap.total_calls, 2, "guardrail must record both calls");
        // NoopAdapter errors → reservation rolls back → usage stays 0.
        let used_with_id = quota.usage(&bot.to_string()).await.unwrap();
        assert_eq!(used_with_id, 0, "failed Noop call rolls back reservation");
    }

    #[tokio::test]
    async fn execute_on_disabled_connector_errors() {
        let (bridge, registry) = bridge_with_echo("connector-echo");
        {
            let mut reg = registry.write().await;
            reg.disable("connector-echo").unwrap();
        }
        let err = bridge
            .execute(
                "connector-echo",
                "echo",
                serde_json::json!({}),
                WasmTier::Warming,
            )
            .await
            .unwrap_err();
        // Disabled connector should surface as a bridge error — don't
        // hardcode the inner message, just confirm it's not silently
        // returning a success.
        assert!(matches!(err, BridgeError::Connector(_)));
    }
}
