//! End-to-end tests for Phase 1a subsystems without HTTP.
//!
//! These test the rule engine, cron scheduler, and manifest signature
//! verification as integrated subsystems.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use springtale_connector::manifest::types::{Capability, ConnectorManifest};
use springtale_connector::manifest::verify::{verify_manifest, verify_manifest_signature};
use springtale_core::router::dispatch::dispatch_event;
use springtale_core::rule::action::Action;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_core::rule::trigger::Trigger;
use springtale_core::rule::types::{Rule, RuleId, RuleStatus, RuleVersion};
use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::signature::sign::sign_canonical_json;
use springtale_scheduler::cron::executor::CronExecutor;

// ────────────────────────────────────────────────────────────────────────────────
// Rule Engine + Dispatch
// ────────────────────────────────────────────────────────────────────────────────

#[test]
fn test_rule_matches_connector_event() {
    let mut engine = RuleEngine::new();

    let rule = Rule {
        id: RuleId::new(),
        name: "kick-stream-alert".into(),
        description: "Alert when a Kick stream goes live".into(),
        status: RuleStatus::Enabled,
        version: RuleVersion(1),
        trigger: Trigger::ConnectorEvent {
            connector: "connector-kick".into(),
            event: "stream_live".into(),
        },
        conditions: vec![],
        actions: vec![Action::SendMessage {
            text: "Stream is live!".into(),
        }],
    };

    let rule_id = rule.id;
    engine.add_rule(rule);

    let event = TriggerEvent {
        trigger_type: "ConnectorEvent".into(),
        connector: Some("connector-kick".into()),
        event: Some("stream_live".into()),
        payload: json!({"channel": "test-channel", "title": "Test Stream"}),
    };

    let matches = dispatch_event(&engine, &event);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].rule_id, rule_id);
    assert_eq!(matches[0].rule_name, "kick-stream-alert");
    assert_eq!(matches[0].actions.len(), 1);
    assert!(matches!(
        &matches[0].actions[0],
        Action::SendMessage { text } if text == "Stream is live!"
    ));
    assert_eq!(matches[0].payload["channel"], "test-channel");
}

// ────────────────────────────────────────────────────────────────────────────────
// Cron Scheduler
// ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_cron_trigger_fires() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let mut executor = CronExecutor::new(tx);

    // Schedule a cron that fires every second
    executor
        .schedule("every-second", "* * * * * *")
        .unwrap();

    // Wait for at least one event (generous timeout to avoid flakes)
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;

    assert!(result.is_ok(), "timed out waiting for cron trigger event");
    let event = result.unwrap();
    assert!(event.is_some(), "channel closed unexpectedly");

    let event = event.unwrap();
    assert_eq!(event.trigger_type, "Cron");
    assert_eq!(event.event.as_deref(), Some("every-second"));
    assert!(event.payload["fired_at"].is_string());
    assert_eq!(event.payload["schedule_name"], "every-second");

    executor.cancel_all();
}

// ────────────────────────────────────────────────────────────────────────────────
// Manifest Signature Verification
// ────────────────────────────────────────────────────────────────────────────────

fn test_manifest() -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-test".into(),
        version: "1.0.0".into(),
        author: "test-author".into(),
        description: "A test connector for signature verification".into(),
        capabilities: vec![Capability::NetworkOutbound {
            host: "api.example.com".into(),
        }],
        triggers: vec![],
        actions: vec![],
        data_disclosure: vec![],
        wasm_hash: None,
        signature: None,
    }
}

/// Build the signable JSON (all fields except signature) and sign it.
fn sign_manifest(manifest: &mut ConnectorManifest, keypair: &Keypair) {
    let mut json = serde_json::to_value(&*manifest).unwrap();
    if let serde_json::Value::Object(ref mut map) = json {
        map.remove("signature");
    }
    let sig = sign_canonical_json(keypair, &json).unwrap();
    manifest.signature = Some(hex::encode(sig.to_bytes()));
}

#[test]
fn test_manifest_signature_verification() {
    let keypair = Keypair::generate().unwrap();
    let mut manifest = test_manifest();

    // Unsigned manifest passes structure check
    assert!(verify_manifest(&manifest).is_ok());

    // Sign the manifest
    sign_manifest(&mut manifest, &keypair);

    // Valid signature passes
    let result = verify_manifest_signature(&manifest, keypair.verifying_key());
    assert!(result.is_ok(), "valid signature should verify: {result:?}");

    // Tamper with the description
    manifest.description = "tampered description".into();

    // Tampered manifest fails signature check
    let result = verify_manifest_signature(&manifest, keypair.verifying_key());
    assert!(
        result.is_err(),
        "tampered manifest should fail signature verification"
    );
}
