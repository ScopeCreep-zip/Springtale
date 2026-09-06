//! The MCP endpoint is not a bypass.
//!
//! Plan 6.5: `call_tool` is one
//! `dispatch::dispatch_action(Action::RunConnector { .. })`, so the
//! sentinel, the approval gate and the executions recorder see an MCP
//! call exactly as they see a rule action. This test proves the sentinel
//! half: when the sentinel refuses the action, the connector is never
//! executed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use springtale_connector::connector::trait_::{ActionResult, EventHandler};
use springtale_connector::manifest::SignatureAlgorithm;
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};
use springtale_connector::{Connector, ConnectorError, Subscription, SubscriptionId};
use springtale_mcp::SpringtaleMcp;
use springtaled::test_harness::TestApp;

const CONNECTOR: &str = "connector-counting";

/// A connector that records how many times it was actually executed.
struct CountingConnector {
    manifest: ConnectorManifest,
    calls: Arc<AtomicUsize>,
}

impl CountingConnector {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self {
            calls,
            manifest: ConnectorManifest {
                name: CONNECTOR.into(),
                version: "1.0.0".into(),
                author: "test".into(),
                description: "counts executions".into(),
                // No capabilities: the capability checker cannot be what
                // refuses the call, so a refusal is the sentinel's.
                capabilities: vec![],
                triggers: vec![],
                actions: vec![ActionDecl {
                    read_only: true,
                    destructive: None,
                    poll_interval_secs: None,
                    name: "peek".into(),
                    description: "Peek at nothing".into(),
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
impl Connector for CountingConnector {
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ActionResult {
            success: true,
            output: serde_json::json!({ "action": action }),
            message: "executed".into(),
        })
    }
    async fn on_event(
        &self,
        _trigger: &str,
        _handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        Ok(Subscription {
            id: SubscriptionId(0),
            trigger: String::new(),
        })
    }
    async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
        Ok(())
    }
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

/// Install the counting connector and hand back the MCP server plus the
/// execution counter.
async fn setup() -> (TestApp, SpringtaleMcp, Arc<AtomicUsize>) {
    let app = TestApp::build(true);
    let calls = Arc::new(AtomicUsize::new(0));
    app.state
        .runtime
        .registry
        .write()
        .await
        .install_native(Box::new(CountingConnector::new(calls.clone())))
        .expect("install test connector");
    let mcp = SpringtaleMcp::new(app.state.runtime.clone());
    (app, mcp, calls)
}

#[tokio::test]
async fn test_mcp_tool_call_runs_the_connector_when_the_sentinel_allows() {
    let (_app, mcp, calls) = setup().await;

    let result = mcp
        .dispatch_tool(&format!("{CONNECTOR}.peek"), serde_json::json!({}))
        .await
        .expect("dispatch");

    assert_ne!(result.is_error, Some(true), "sentinel should have allowed");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "connector ran exactly once"
    );
}

#[tokio::test]
async fn test_mcp_tool_call_denied_by_sentinel_does_not_execute() {
    let (app, mcp, calls) = setup().await;

    // Trip the circuit breaker (default threshold: 3 consecutive
    // failures). Nothing about MCP is involved — this is the sentinel
    // state a rule action would hit too.
    for _ in 0..3 {
        app.state.runtime.sentinel.report_failure(CONNECTOR);
    }

    let result = mcp
        .dispatch_tool(&format!("{CONNECTOR}.peek"), serde_json::json!({}))
        .await
        .expect("dispatch returns a tool result, not a protocol error");

    assert_eq!(
        result.is_error,
        Some(true),
        "a sentinel-refused MCP call must come back as a tool error"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the connector must never be executed when the sentinel refuses"
    );
}

#[tokio::test]
async fn test_mcp_tools_are_named_connector_dot_action() {
    let (_app, mcp, _calls) = setup().await;
    let tools = mcp.tools().await;
    let tool = tools
        .iter()
        .find(|t| t.name.as_ref() == format!("{CONNECTOR}.peek"))
        .expect("qualified tool name");
    let annotations = tool.annotations.as_ref().expect("annotations");
    assert_eq!(annotations.read_only_hint, Some(true));
    assert_eq!(annotations.destructive_hint, Some(false));
}
