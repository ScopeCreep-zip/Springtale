//! End-to-end tests for Phase 1a subsystems without HTTP.
//!
//! These test the rule engine, cron scheduler, and manifest signature
//! verification as integrated subsystems.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use springtale_connector::manifest::SignatureAlgorithm;
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
use springtale_store::StorageBackend;
use springtale_store::backend::InMemoryBackend;

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
        owner: Default::default(),
    };

    let rule_id = rule.id;
    engine.add_rule(rule).expect("failed to add rule");

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
async fn test_cron_scheduling_and_cancellation() {
    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    let mut executor = CronExecutor::new(tx);

    // Valid expression (every minute) should succeed
    executor.schedule("every-minute", "0 * * * * *").unwrap();
    assert_eq!(executor.list(), vec!["every-minute"]);

    // Per-second expression should be rejected by minimum interval check
    let result = executor.schedule("spam", "* * * * * *");
    assert!(result.is_err(), "should reject per-second cron");

    // Verify next fire time is calculable
    let next = executor.next_fire_for("every-minute");
    assert!(next.is_some(), "should have a next fire time");

    // Cancel and verify
    assert!(executor.cancel("every-minute"));
    assert!(executor.list().is_empty());

    executor.cancel_all();
}

#[tokio::test]
async fn test_cron_rejects_too_frequent() {
    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    let mut executor = CronExecutor::new(tx);

    // Every 30 seconds: rejected
    assert!(executor.schedule("fast", "0,30 * * * * *").is_err());

    // Every 5 minutes: accepted
    assert!(executor.schedule("ok", "0 */5 * * * *").is_ok());

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
        roles: vec![],
        wasm_hash: None,
        signature_alg: SignatureAlgorithm::default(),
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

// ────────────────────────────────────────────────────────────────────────────────
// Full Pipeline: Rule → Dispatch → Action Resolution
// ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_full_rule_pipeline_with_store() {
    // 1. Initialize in-memory store
    let store: std::sync::Arc<dyn StorageBackend> = std::sync::Arc::new(InMemoryBackend::new());

    // 2. Create a rule and persist it
    let rule = Rule {
        id: RuleId::new(),
        name: "github-webhook-notify".into(),
        description: "When GitHub push event fires, send a message".into(),
        status: RuleStatus::Enabled,
        version: RuleVersion(1),
        trigger: Trigger::ConnectorEvent {
            connector: "connector-github".into(),
            event: "push".into(),
        },
        conditions: vec![],
        actions: vec![Action::SendMessage {
            text: "New push to repo".into(),
        }],
        owner: Default::default(),
    };

    let rule_id = rule.id;

    // Persist to store
    store.insert_rule(&rule).await.expect("insert rule failed");

    // 3. Load rules from store into engine (same as runtime boot)
    let rules = store.list_rules().await.expect("list rules failed");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "github-webhook-notify");

    let mut engine = RuleEngine::new();
    for r in &rules {
        engine
            .add_rule(r.clone())
            .expect("add rule to engine failed");
    }

    // 4. Simulate a trigger event
    let event = TriggerEvent {
        trigger_type: "ConnectorEvent".into(),
        connector: Some("connector-github".into()),
        event: Some("push".into()),
        payload: json!({
            "ref": "refs/heads/main",
            "repository": { "full_name": "user/repo" },
            "pusher": { "name": "developer" }
        }),
    };

    // 5. Dispatch event through engine
    let matches = dispatch_event(&engine, &event);
    assert_eq!(matches.len(), 1, "expected exactly one rule match");
    assert_eq!(matches[0].rule_id, rule_id);
    assert_eq!(matches[0].actions.len(), 1);

    // 6. Log event to store (same as webhook handler does)
    let event_entry = springtale_store::schema::events::EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: "connector-github".into(),
        trigger_type: "push".into(),
        timestamp: chrono::Utc::now(),
        action_taken: "SendMessage: New push to repo".into(),
    };
    store
        .log_event(&event_entry)
        .await
        .expect("log event failed");

    // 7. Verify event was persisted
    let events = store
        .list_events(&springtale_store::schema::events::EventFilter {
            connector_name: Some("connector-github".into()),
            ..Default::default()
        })
        .await
        .expect("list events failed");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].connector_name, "connector-github");
    assert_eq!(events[0].trigger_type, "push");

    // 8. Verify rule can be toggled via store
    store
        .toggle_rule(&rule_id, false)
        .await
        .expect("toggle failed");

    let updated = store.list_rules().await.expect("list after toggle failed");
    assert_eq!(updated[0].status, RuleStatus::Disabled);

    // 9. Disabled rule should NOT match
    let mut engine2 = RuleEngine::new();
    for r in &updated {
        engine2
            .add_rule(r.clone())
            .expect("add rule to engine failed");
    }
    let matches2 = dispatch_event(&engine2, &event);
    assert_eq!(matches2.len(), 0, "disabled rule should not match");

    // 10. Verify rule can be deleted
    store.delete_rule(&rule_id).await.expect("delete failed");

    let remaining = store.list_rules().await.expect("list after delete failed");
    assert!(remaining.is_empty(), "rule should be deleted");
}

#[tokio::test]
async fn test_connector_lifecycle_with_store() {
    let store: std::sync::Arc<dyn StorageBackend> = std::sync::Arc::new(InMemoryBackend::new());

    // 1. Register connector manifest
    let row = springtale_store::schema::connectors::ConnectorRow {
        name: "connector-test".into(),
        version: "1.0.0".into(),
        author: "test".into(),
        description: "integration test connector".into(),
        manifest_json: "{}".into(),
        enabled: true,
        installed_at: chrono::Utc::now(),
    };
    store
        .register_connector(&row)
        .await
        .expect("register failed");

    // 2. Verify it's listed
    let connectors = store.list_connectors().await.expect("list failed");
    assert_eq!(connectors.len(), 1);
    assert_eq!(connectors[0].name, "connector-test");

    // 3. Disable
    store
        .set_connector_enabled("connector-test", false)
        .await
        .expect("disable failed");
    let connectors = store.list_connectors().await.expect("list after disable");
    assert!(!connectors[0].enabled);

    // 4. Remove
    store
        .remove_connector("connector-test")
        .await
        .expect("remove failed");
    let connectors = store.list_connectors().await.expect("list after remove");
    assert!(connectors.is_empty());
}
