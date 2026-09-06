//! Phase-7 audit Finding A end-to-end regression.
//!
//! Proves the full ShellExec gate-mediated flow:
//!
//! 1. A connector declaring `Capability::ShellExec` is registered
//!    under `CapabilityPolicy::AllowAll` (the documented "not
//!    recommended" but shipped policy). The capability lands in
//!    `pending_approval` per Phase-7 Finding A, NOT in `approved`.
//! 2. `bridge.execute()` for an action on that connector fires the
//!    `ApprovalGate`. The gate blocks until the management API
//!    resolves it (we drive both sides from the test).
//! 3. Approve → call proceeds → `ActionResult` returned.
//!    Deny → `BridgeError::ApprovalDenied`.
//!    Timeout → `BridgeError::ApprovalTimedOut`.
//!
//! The hostile-connector scenario this defeats: a malicious community
//! connector declares ShellExec and tries to run arbitrary shell
//! commands as soon as a rule fires. The gate stops the dispatch
//! before any code under the connector's control runs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
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
use springtale_runtime::{
    ApprovalDecision, ApprovalGate, ApprovalRequestId, BridgeError, CapabilityBridge,
    DefaultDenyApprovalGate,
};

/// Connector that declares ShellExec — the threat-model
/// representative for OpenClaw CVE-2026-25253. Doesn't actually
/// touch the shell; the test asserts the gate fires BEFORE the
/// connector code runs.
struct ShellLikeConnector {
    manifest: ConnectorManifest,
}

impl ShellLikeConnector {
    fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                name: "connector-shell-like".into(),
                version: "0.1.0".into(),
                author: "test".into(),
                description: "declares ShellExec for gate-test purposes".into(),
                capabilities: vec![Capability::ShellExec],
                triggers: vec![TriggerDecl {
                    name: "ping".into(),
                    description: "ping".into(),
                    schema: None,
                }],
                actions: vec![ActionDecl {
                    name: "exec".into(),
                    description: "would exec a shell command".into(),
                    input_schema: None,
                    output_schema: None,
                    read_only: false,
                    destructive: None,
                    poll_interval_secs: None,
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
impl Connector for ShellLikeConnector {
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
            output: json!({ "executed": action, "input": input }),
            message: "would have shelled out, gate let us through".into(),
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

fn bridge_with_gate(gate: Arc<dyn springtale_runtime::ApprovalGate>) -> CapabilityBridge {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(ShellLikeConnector::new()))
        .unwrap();
    CapabilityBridge::new(Arc::new(RwLock::new(registry))).with_approval_gate(gate)
}

#[tokio::test]
async fn shell_exec_call_blocks_then_proceeds_on_approve() {
    let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(Duration::from_secs(
        5,
    )));
    let bridge = bridge_with_gate(gate.clone() as Arc<dyn springtale_runtime::ApprovalGate>);

    // Spawn the dispatch — it blocks on the gate until we resolve.
    let bridge2 = bridge.clone();
    let handle = tokio::spawn(async move {
        bridge2
            .execute(
                "connector-shell-like",
                "exec",
                json!({ "cmd": "echo hello" }),
                WasmTier::Warming,
            )
            .await
    });

    // Wait for the request to land in the gate's pending queue.
    let id = wait_for_pending(&gate).await;

    // User approves.
    gate.resolve(
        id,
        ApprovalDecision::Approved {
            approver: "test-user".into(),
            approved_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = handle.await.unwrap().expect("approved call must succeed");
    assert!(result.success);
    assert_eq!(result.output["executed"], "exec");
}

#[tokio::test]
async fn shell_exec_call_fails_on_deny() {
    let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(Duration::from_secs(
        5,
    )));
    let bridge = bridge_with_gate(gate.clone() as Arc<dyn springtale_runtime::ApprovalGate>);

    let bridge2 = bridge.clone();
    let handle = tokio::spawn(async move {
        bridge2
            .execute(
                "connector-shell-like",
                "exec",
                json!({ "cmd": "rm -rf /" }),
                WasmTier::Warming,
            )
            .await
    });

    let id = wait_for_pending(&gate).await;

    gate.resolve(
        id,
        ApprovalDecision::Denied {
            reason: "obviously not".into(),
            denied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let err = handle.await.unwrap().expect_err("denied call must fail");
    match err {
        BridgeError::ApprovalDenied(reason) => assert_eq!(reason, "obviously not"),
        other => panic!("expected ApprovalDenied, got {other:?}"),
    }
}

#[tokio::test]
async fn shell_exec_call_fails_on_timeout() {
    // 100ms gate timeout — no user resolution arrives — must surface
    // BridgeError::ApprovalTimedOut. This is the safety property when
    // the maintainer is asleep / offline.
    let gate = Arc::new(DefaultDenyApprovalGate::with_timeout(
        Duration::from_millis(100),
    ));
    let bridge = bridge_with_gate(gate as Arc<dyn springtale_runtime::ApprovalGate>);

    let err = bridge
        .execute(
            "connector-shell-like",
            "exec",
            json!({ "cmd": "anything" }),
            WasmTier::Warming,
        )
        .await
        .expect_err("timeout must surface as deny-fallback");
    assert!(
        matches!(err, BridgeError::ApprovalTimedOut { .. }),
        "expected ApprovalTimedOut, got {err:?}"
    );
}

#[tokio::test]
async fn shell_exec_fails_when_no_gate_wired() {
    // No `with_approval_gate` call → the bridge must refuse rather
    // than silently degrade to "no gate = allow". Fails closed.
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(ShellLikeConnector::new()))
        .unwrap();
    let bridge = CapabilityBridge::new(Arc::new(RwLock::new(registry)));

    let err = bridge
        .execute("connector-shell-like", "exec", json!({}), WasmTier::Warming)
        .await
        .expect_err("missing gate must fail closed for ShellExec");
    assert!(
        matches!(err, BridgeError::ApprovalGate(_)),
        "expected ApprovalGate (no gate wired), got {err:?}"
    );
}

/// Helper: spin until the gate has at least one pending request,
/// return its id. Bounded so a regression that fails to register
/// the pending request surfaces as a test timeout, not a hang.
async fn wait_for_pending(gate: &DefaultDenyApprovalGate) -> ApprovalRequestId {
    for _ in 0..100 {
        let pending = gate.pending().await;
        if let Some(p) = pending.first() {
            return p.id;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("approval request never landed in gate's pending queue");
}
