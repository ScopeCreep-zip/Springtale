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

use serde_json::Value;
use thiserror::Error;
use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::tier::WasmTier;
use springtale_connector::ActionResult;
use springtale_cooperation::momentum::MomentumTier;

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

/// Errors surfaced by the bridge. Wraps connector errors so callers in
/// the bot event loop don't need to import the connector error module.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("connector error: {0}")]
    Connector(#[from] springtale_connector::ConnectorError),
}

/// Bridge between formation momentum and connector execution.
///
/// Clone is cheap (Arc inside). Held by `RuntimeState` so bot event
/// loops and external entry points share the same dispatch path.
#[derive(Clone)]
pub struct CapabilityBridge {
    registry: Arc<RwLock<ConnectorRegistry>>,
}

impl CapabilityBridge {
    /// Create a bridge around the shared connector registry.
    pub fn new(registry: Arc<RwLock<ConnectorRegistry>>) -> Self {
        Self { registry }
    }

    /// Access the underlying connector registry. Exposed so callers
    /// holding only a `&CapabilityBridge` (e.g. `dispatch_action`) can
    /// still reach the registry for non-RunConnector code paths — the
    /// bridge is the single public entry point for connector-call
    /// routing but the registry is the authoritative store.
    pub fn registry(&self) -> &Arc<RwLock<ConnectorRegistry>> {
        &self.registry
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
        let (host, checker) = {
            let registry = self.registry.read().await;
            registry.get_for_execute(connector_name)?
        };
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
        self.execute(connector_name, action, input, momentum_to_wasm_tier(momentum))
            .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use springtale_connector::capability::grant::CapabilityPolicy;
    use springtale_connector::connector::subscription::{Subscription, SubscriptionId};
    use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
    use springtale_connector::manifest::types::{
        ActionDecl, Capability, ConnectorManifest, TriggerDecl,
    };
    use springtale_connector::ConnectorError;

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
                        name: "echo".into(),
                        description: "echo".into(),
                        input_schema: None,
                        output_schema: None,
                    }],
                    data_disclosure: vec![],
                    roles: vec![],
                    wasm_hash: None,
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
            .execute("does-not-exist", "noop", serde_json::json!({}), WasmTier::Cold)
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
