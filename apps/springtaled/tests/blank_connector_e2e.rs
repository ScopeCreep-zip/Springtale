//! F-conn-5 — Blank-connector E2E smoke test.
//!
//! Validates that a brand-new connector with no special-cased name
//! anywhere in the codebase can be installed, listed, dispatched, and
//! reloaded purely through the universal interface. If this test ever
//! starts failing, it means F-conn-1's "no hardcoded connector names
//! outside the connector's own crate" invariant has regressed.
//!
//! The blank connector here implements `springtale_connector::Connector`
//! with one trigger + one action — the minimum surface a community
//! connector ships. It deliberately uses a name that doesn't appear
//! anywhere else in the workspace (`connector-blank-smoke-test`) so
//! any code that branches on connector name will fall through to the
//! universal path.
//!
//! The test exercises:
//! - registry install (`install_native`)
//! - registry list (`list_connectors`)
//! - registry execute (`execute`)
//! - registry reload (`reload_connector` — G4)
//! - registry remove (`remove_connector`)
//!
//! Per `COOPERATION_IMPLEMENTATION_PLAN.md §8`, this is the "blank-bot
//! template" the documentation references — a literal end-to-end run
//! through the connector contract with zero hidden assumptions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use async_trait::async_trait;
use std::sync::Arc;

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::connector::subscription::{Subscription, SubscriptionId};
use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::SignatureAlgorithm;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, TriggerDecl,
};
use springtale_connector::registry::store::ConnectorRegistry;

const BLANK_NAME: &str = "connector-blank-smoke-test";

struct BlankConnector {
    manifest: ConnectorManifest,
    invocation_counter: Arc<std::sync::atomic::AtomicU32>,
}

impl BlankConnector {
    fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                name: BLANK_NAME.to_owned(),
                version: "0.0.1".into(),
                author: "smoke-test".into(),
                description: "blank connector for F-conn-5 universality smoke test".into(),
                capabilities: vec![Capability::NetworkOutbound {
                    host: "example.invalid".into(),
                }],
                triggers: vec![TriggerDecl {
                    name: "tick".into(),
                    description: "blank-tick: emits whenever a host pokes it".into(),
                    schema: None,
                }],
                actions: vec![ActionDecl {
                    read_only: false,
                    name: "echo".into(),
                    description: "blank-echo: returns the input verbatim".into(),
                    input_schema: None,
                    output_schema: None,
                }],
                data_disclosure: vec![],
                roles: vec![],
                wasm_hash: None,
                signature_alg: SignatureAlgorithm::default(),
                signature: None,
            },
            invocation_counter: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
}

#[async_trait]
impl Connector for BlankConnector {
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
        self.invocation_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if action != "echo" {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown action: {action}"
            )));
        }
        Ok(ActionResult {
            success: true,
            output: input,
            message: "blank-echo ok".into(),
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

/// Full lifecycle round-trip: install → list → execute → list-after.
/// Every interaction goes through the universal `ConnectorRegistry`
/// API; no code path knows or cares that the connector is named
/// `connector-blank-smoke-test`.
#[tokio::test]
async fn blank_connector_install_list_execute_roundtrip() {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);

    let registered = registry
        .install_native(Box::new(BlankConnector::new()))
        .expect("install_native must accept a fresh connector via the universal path");
    assert_eq!(
        registered, BLANK_NAME,
        "install_native must return the connector's own manifest name verbatim"
    );

    // list_connectors surfaces the new entry without any name special-casing.
    let listed = registry.list();
    assert!(
        listed
            .iter()
            .any(|(name, enabled)| *name == BLANK_NAME && *enabled),
        "blank connector should appear in the registry list as enabled"
    );

    // Universal dispatch — `execute` doesn't know which connector this is.
    let result = registry
        .execute(BLANK_NAME, "echo", serde_json::json!({"hello": "world"}))
        .await
        .expect("blank-echo must dispatch through the universal execute path");
    assert!(result.success);
    assert_eq!(result.output, serde_json::json!({"hello": "world"}));
}

/// G4 path — `reload` swaps the entry's host atomically. Verifies the
/// universal reload primitive works for a connector the framework has
/// never seen before; the entry is replaced and a subsequent `execute`
/// lands on the fresh instance (proven by the invocation counter on the
/// new connector starting at 0).
#[tokio::test]
async fn blank_connector_reload_atomic_swap() {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    let first = BlankConnector::new();
    let first_counter = first.invocation_counter.clone();

    registry.install_native(Box::new(first)).expect("install");

    // First call bumps the original counter.
    registry
        .execute(BLANK_NAME, "echo", serde_json::json!({"n": 1}))
        .await
        .expect("first execute");
    assert_eq!(
        first_counter.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "first connector instance saw exactly one call"
    );

    // Hot-reload with a fresh BlankConnector — same manifest name,
    // brand-new instance + counter.
    let second = BlankConnector::new();
    let second_counter = second.invocation_counter.clone();
    let new_host: Arc<dyn springtale_connector::host::ConnectorHost> = Arc::new(
        springtale_connector::native::runtime::NativeConnectorHost::new(Box::new(second))
            .expect("native host wrap"),
    );
    let old_host = registry
        .reload(BLANK_NAME, new_host)
        .expect("reload must swap atomically");

    // Drop the old host explicitly so subsequent execute lands on the
    // new instance (mirrors `operations::connectors::reload_connector`
    // which drops `old_host` after the swap).
    drop(old_host);

    registry
        .execute(BLANK_NAME, "echo", serde_json::json!({"n": 2}))
        .await
        .expect("post-reload execute");
    // Original counter stays at 1 — the new call landed on the swapped instance.
    assert_eq!(
        first_counter.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "old instance must not see post-reload calls"
    );
    assert_eq!(
        second_counter.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "new instance must receive the post-reload call"
    );
}

/// Unknown actions must fail through the same universal error path —
/// no connector-specific branch is required to surface "unknown action"
/// to the caller.
#[tokio::test]
async fn blank_connector_unknown_action_returns_error() {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(BlankConnector::new()))
        .expect("install");
    let err = registry
        .execute(BLANK_NAME, "does-not-exist", serde_json::Value::Null)
        .await
        .expect_err("unknown action must error through the universal path");
    let msg = err.to_string();
    assert!(
        msg.contains("does-not-exist") || msg.contains("unknown"),
        "error message must reference the unknown action: {msg}"
    );
}

/// Disabling the connector through the universal `disable` path must
/// block subsequent execute calls — no connector-name branch decides
/// enable/disable semantics.
#[tokio::test]
async fn blank_connector_disable_blocks_execute() {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(BlankConnector::new()))
        .expect("install");
    registry.disable(BLANK_NAME).expect("disable");
    let err = registry
        .execute(BLANK_NAME, "echo", serde_json::Value::Null)
        .await
        .expect_err("disabled connector must refuse execute");
    assert!(err.to_string().contains("disabled"));

    // Re-enable + execute succeeds → enable/disable is genuinely
    // bidirectional, not a one-way flag.
    registry.enable(BLANK_NAME).expect("re-enable");
    registry
        .execute(BLANK_NAME, "echo", serde_json::json!({}))
        .await
        .expect("post-reenable execute");
}

/// Remove path — universal `remove` drops the entry; subsequent
/// operations error with `NotFound`.
#[tokio::test]
async fn blank_connector_remove_clears_registry_entry() {
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(BlankConnector::new()))
        .expect("install");
    assert!(registry.get(BLANK_NAME).is_some());

    registry.remove(BLANK_NAME).expect("remove");
    assert!(registry.get(BLANK_NAME).is_none());

    let err = registry
        .execute(BLANK_NAME, "echo", serde_json::Value::Null)
        .await
        .expect_err("removed connector must surface NotFound");
    assert!(err.to_string().to_lowercase().contains("not found"));
}
