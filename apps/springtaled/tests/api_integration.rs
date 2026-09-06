//! API integration tests for the springtaled management API.
//!
//! These test the full HTTP layer (router + middleware + handlers) using
//! `tower::ServiceExt::oneshot()` — no TCP socket required.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tokio::sync::{Mutex, RwLock, mpsc};
use tower::ServiceExt;

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::RuleEngine;
use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;
use springtale_store::backend::sqlite::SqliteBackend;
use springtaled::api::build_router;
use springtaled::api::state::AppState;

use springtale_crypto::token::derive_api_token_hash;

/// Build a test router with in-memory state. Returns (Router, hex-encoded token).
///
/// The `ready` flag defaults to `true`. Pass `false` for tests that need
/// the daemon to appear as still booting.
fn build_test_app(ready: bool) -> (Router, String) {
    let store: Arc<dyn springtale_store::StorageBackend> =
        Arc::new(SqliteBackend::open_in_memory().unwrap());
    let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
        CapabilityPolicy::AllowAll,
    )));
    let engine = Arc::new(RwLock::new(RuleEngine::new()));

    let passphrase = b"test-passphrase";
    let api_token_hash = derive_api_token_hash(passphrase);
    let token_hex = hex::encode(api_token_hash);

    let (trigger_tx, _trigger_rx) = mpsc::channel(256);
    let cron = Arc::new(Mutex::new(CronExecutor::new(trigger_tx.clone())));
    let fs_watcher = Arc::new(Mutex::new(FsWatcher::new(trigger_tx.clone()).unwrap()));

    let ready_flag = Arc::new(AtomicBool::new(ready));

    let sentinel = Arc::new(springtale_sentinel::Sentinel::new(
        springtale_sentinel::SentinelConfig::default(),
        store.clone(),
    ));

    let ai_adapter = Arc::new(arc_swap::ArcSwap::from(Arc::new(
        Arc::new(springtale_ai::NoopAdapter) as Arc<dyn springtale_ai::AiAdapter>,
    )));

    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let (bot_msg_tx, _bot_msg_rx) = mpsc::channel(256);
    let (chat_tx, _chat_rx) = tokio::sync::broadcast::channel(256);
    let trigger_registry =
        springtale_runtime::TriggerRegistry::new(trigger_tx.clone(), store.clone());

    let heartbeat_monitor = std::sync::Arc::new(tokio::sync::Mutex::new(
        springtale_scheduler::HeartbeatMonitor::new(0, trigger_tx.clone()),
    ));

    let canvas = std::sync::Arc::new(tokio::sync::RwLock::new(
        springtale_core::canvas::CanvasState::default(),
    ));
    let (canvas_tx, _rx) = tokio::sync::broadcast::channel(64);
    let (notification_tx, _notif_rx) = tokio::sync::broadcast::channel(256);
    let (cooperation_tx, _coop_rx) = tokio::sync::broadcast::channel(512);
    let formation_gossip: std::sync::Arc<dyn springtale_cooperation::gossip::FormationGossipBus> =
        springtale_cooperation::gossip::InMemoryFormationGossipBus::new();
    let knowledge_store: std::sync::Arc<dyn springtale_cooperation::memory::GlobalKnowledgeStore> =
        springtale_cooperation::memory::InMemoryKnowledgeStore::new();

    let wasm_engine = std::sync::Arc::new(
        springtale_connector::wasm::WasmEngine::new(
            springtale_connector::wasm::SandboxLimits::default(),
        )
        .expect("WASM engine creation"),
    );
    let wasm_tier_cache = std::sync::Arc::new(
        springtale_connector::wasm::WasmTierCache::new(wasm_engine.clone())
            .expect("WASM tier cache init"),
    );

    let (formation_cmd_tx, _formation_cmd_rx) =
        mpsc::channel::<springtale_cooperation::command::FormationCommand>(32);

    let gossip_store: std::sync::Arc<dyn springtale_cooperation::awareness::GossipStore> =
        std::sync::Arc::new(springtale_cooperation::awareness::InMemoryGossipStore::new());
    let capability_bridge = springtale_runtime::CapabilityBridge::new(registry.clone());
    let role_registry =
        std::sync::Arc::new(springtale_cooperation::role::RoleRegistry::with_builtins());
    let runtime = springtale_runtime::RuntimeState {
        store,
        registry,
        engine,
        ai_adapter,
        sentinel,
        wasm_engine,
        wasm_tier_cache,
        capability_bridge,
        role_registry,
        canvas,
        canvas_tx,
        notification_tx,
        event_tx: event_tx.clone(),
        trigger_registry: std::sync::Arc::new(std::sync::OnceLock::new()),
        cooperation_tx,
        utterances: Default::default(),
        utterance_defs: Default::default(),
        cadence_tick: Default::default(),
        formation_cmd_tx,
        live_formations: None,
        gossip_store,
        formation_gossip,
        knowledge_store,
        // Single-process test fixture — no SWIM node.
        swim_node: None,
        // In-memory store — no runtime lock.
        _lock: None,
    };

    // EmbeddedScheduler replaced the old `springtaled::scheduler::AppScheduler`
    // when scheduling moved into the runtime crate so the desktop app could
    // share it. The producer is a stub — integration tests don't drive the
    // job consumer end-to-end, only the API surface.
    let (job_tx, _job_rx) = mpsc::channel::<springtale_scheduler::Job>(16);
    let producer = Arc::new(springtale_scheduler::JobProducer::new(job_tx));
    let scheduler = springtale_runtime::EmbeddedScheduler {
        cron,
        fs_watcher,
        trigger_tx: trigger_tx.clone(),
        producer,
    };

    let state = AppState {
        runtime,
        api_token_hash,
        ready: ready_flag,
        trigger_tx,
        scheduler,
        rate_limit_per_sec: 1000,
        event_tx,
        heartbeat_monitor,
        bot_msg_tx,
        trigger_registry,
        chat_tx,
        stream_tickets: Arc::new(Mutex::new(HashMap::new())),
    };

    let router = build_router(state);
    (router, token_hex)
}

/// Helper: send a request through the router.
async fn send(router: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ────────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_health_returns_200() {
    let (router, _token) = build_test_app(true);
    let req = Request::get("/health").body(Body::empty()).unwrap();
    let (status, body) = send(router, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn test_ready_returns_200() {
    let (router, _token) = build_test_app(true);
    let req = Request::get("/ready").body(Body::empty()).unwrap();
    let (status, body) = send(router, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");
    assert_eq!(body["store"], "ok");
}

#[tokio::test]
async fn test_ready_returns_503_before_boot() {
    let (router, _token) = build_test_app(false);
    let req = Request::get("/ready").body(Body::empty()).unwrap();
    let (status, body) = send(router, req).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "booting");
}

#[tokio::test]
async fn test_unauthenticated_returns_401() {
    let (router, _token) = build_test_app(true);

    // No Authorization header
    let req = Request::get("/connectors").body(Body::empty()).unwrap();
    let (status, _body) = send(router, req).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_authenticated_connectors_list() {
    let (router, token) = build_test_app(true);

    let req = Request::get("/connectors")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(router, req).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["connectors"].is_array());
    assert_eq!(body["connectors"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_and_list_rules() {
    let (router, token) = build_test_app(true);

    // Create a rule
    let rule_json = serde_json::json!({
        "name": "test-rule",
        "description": "integration test rule",
        "status": "enabled",
        "version": 1,
        "trigger": {
            "type": "Webhook",
            "path": "my-hook"
        },
        "conditions": [],
        "actions": [
            { "type": "SendMessage", "text": "hello from test" }
        ]
    });

    let create_req = Request::post("/rules")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&rule_json).unwrap()))
        .unwrap();

    let (status, create_body) = send(router.clone(), create_req).await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(create_body["id"].is_string());

    // List rules and verify the created rule appears
    let list_req = Request::get("/rules")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let (status, list_body) = send(router, list_req).await;

    assert_eq!(status, StatusCode::OK);
    let rules = list_body["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["name"], "test-rule");
}

#[tokio::test]
async fn test_delete_rule() {
    let (router, token) = build_test_app(true);

    // Create a rule first
    let rule_json = serde_json::json!({
        "name": "to-delete",
        "description": "will be deleted",
        "status": "enabled",
        "version": 1,
        "trigger": {
            "type": "Webhook",
            "path": "delete-hook"
        },
        "conditions": [],
        "actions": [
            { "type": "SendMessage", "text": "ephemeral" }
        ]
    });

    let create_req = Request::post("/rules")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&rule_json).unwrap()))
        .unwrap();

    let (status, create_body) = send(router.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let rule_id = create_body["id"].as_str().unwrap().to_owned();

    // Delete the rule
    let delete_req = Request::delete(format!("/rules/{rule_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let (status, delete_body) = send(router.clone(), delete_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(delete_body["deleted"], rule_id);

    // Verify the rule list is now empty
    let list_req = Request::get("/rules")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let (status, list_body) = send(router, list_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_body["rules"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_webhook_endpoint() {
    let (router, token) = build_test_app(true);

    // Webhook requires the connector to exist in the registry. Since the
    // registry is empty, we expect a 404 for an unknown connector.
    let webhook_req = Request::post("/webhook/connector-kick/stream_live")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(b"{}".as_slice()))
        .unwrap();

    let (status, _body) = send(router, webhook_req).await;

    // The connector is not in the registry, so the webhook handler returns 404.
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── New operation tests (Phase 4) ──────────────────────────────────────────

#[tokio::test]
async fn test_list_intents() {
    let (router, token) = build_test_app(true);
    let req = Request::get("/formations/intents")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    let intents = body["intents"].as_array().unwrap();
    assert_eq!(intents.len(), 4);
    assert_eq!(intents[0]["value"], "Reconnoiter");
    assert_eq!(intents[1]["value"], "Execute");
    assert_eq!(intents[2]["value"], "Stabilize");
    assert_eq!(intents[3]["value"], "Surge");
}

#[tokio::test]
async fn test_deploy_team_creates_rules_and_formation() {
    let (router, token) = build_test_app(true);
    let body = serde_json::json!({
        "name": "Alpha Squad",
        "intent": "Reconnoiter",
        "guard_mode": false,
        "agents": [{
            "connector_name": "connector-test",
            "trigger_name": "event_received",
            "action_connector": "connector-test",
            "action_name": "send_message"
        }]
    });
    let req = Request::post("/formations/deploy-team")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(json["formation_id"].is_string());
    assert!(json["rule_ids"].is_array());
    assert_eq!(json["rule_ids"].as_array().unwrap().len(), 1);

    // Verify formation was created
    let list_req = Request::get("/formations")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, list_body) = send(router.clone(), list_req).await;
    assert_eq!(status, StatusCode::OK);
    let formations = list_body["formations"].as_array().unwrap();
    assert_eq!(formations.len(), 1);
    assert_eq!(formations[0]["name"], "Alpha Squad");
    assert_eq!(formations[0]["status"], "active");
}

/// End-to-end: a NO-AI formation's intent compiles into formation-scoped
/// rules, and cycling the intent regenerates them non-lossily (AUDIT-NOTES §4).
/// Rule names are stamped with the intent (`… (Reconnoiter)`), so we can verify
/// the transformation through the public `/rules` API alone.
#[tokio::test]
async fn test_formation_intent_synthesizes_and_regenerates_rules() {
    let (router, token) = build_test_app(true);

    let body = serde_json::json!({
        "name": "Recon Squad",
        "intent": "Reconnoiter",
        "guard_mode": false,
        "agents": [{
            "connector_name": "connector-test",
            "trigger_name": "event_received",
            "action_connector": "connector-test",
            "action_name": "send_message"
        }]
    });
    let req = Request::post("/formations/deploy-team")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::CREATED);
    let formation_id = json["formation_id"].as_str().unwrap().to_owned();

    async fn squad_rule_names(router: &Router, token: &str) -> Vec<String> {
        let req = Request::get("/rules")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (_, body) = send(router.clone(), req).await;
        body["rules"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["name"].as_str().map(str::to_owned))
            .filter(|n| n.contains("Recon Squad"))
            .collect()
    }

    // Reconnoiter → exactly one formation-scoped, read-only-intent rule.
    let names = squad_rule_names(&router, &token).await;
    assert_eq!(names.len(), 1, "one synthesized rule, got {names:?}");
    assert!(
        names[0].contains("(Reconnoiter)"),
        "rule reflects Reconnoiter intent: {names:?}"
    );

    // Cycle Reconnoiter → Execute: rules regenerate for the new intent.
    let req = Request::post(format!("/formations/{formation_id}/cycle-intent"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["intent"], "Execute");

    // Still exactly one rule (regenerated, not accumulated) — now Execute.
    let names = squad_rule_names(&router, &token).await;
    assert_eq!(
        names.len(),
        1,
        "intent change regenerates, never accumulates: {names:?}"
    );
    assert!(
        names[0].contains("(Execute)"),
        "rule now reflects Execute intent: {names:?}"
    );

    // Dissolve tears the synthesised rules down.
    let req = Request::post(format!("/formations/{formation_id}/dissolve"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    let names = squad_rule_names(&router, &token).await;
    assert!(
        names.is_empty(),
        "dissolve removed formation rules: {names:?}"
    );
}

#[tokio::test]
async fn test_deploy_team_rejects_empty_name() {
    let (router, token) = build_test_app(true);
    let body = serde_json::json!({
        "name": "",
        "intent": "Reconnoiter",
        "guard_mode": false,
        "agents": [{
            "connector_name": "test",
            "trigger_name": "event",
            "action_connector": "test",
            "action_name": "act"
        }]
    });
    let req = Request::post("/formations/deploy-team")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, _) = send(router, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_connector_rule() {
    let (router, token) = build_test_app(true);
    let body = serde_json::json!({
        "name": "Watch Files",
        "trigger_connector": "connector-filesystem",
        "trigger_event": "file_changed",
        "action_connector": "connector-shell",
        "action_name": "exec",
        "conditions": []
    });
    let req = Request::post("/rules/connector")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router, req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn test_step_autonomy_up_down() {
    let (router, token) = build_test_app(true);
    // Autonomy is keyed by rule id; an id needs no engine lookup.
    let rule_id = uuid::Uuid::new_v4();

    // Step down from default "act-autonomously" to "act-with-approval"
    let body = serde_json::json!({ "direction": "down" });
    let req = Request::post(format!("/agents/{rule_id}/autonomy/step"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["level"], "act-with-approval");

    // Step up back to "act-autonomously"
    let body = serde_json::json!({ "direction": "up" });
    let req = Request::post(format!("/agents/{rule_id}/autonomy/step"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["level"], "act-autonomously");
}

#[tokio::test]
async fn test_toggle_formation_guard() {
    let (router, token) = build_test_app(true);

    // Create a formation first
    let create_body = serde_json::json!({
        "name": "Guard Test",
        "intent": "Stabilize",
        "connectors": []
    });
    let req = Request::post("/formations")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::CREATED);
    let formation_id = json["id"].as_str().unwrap().to_owned();

    // Toggle guard on
    let req = Request::post(format!("/formations/{formation_id}/toggle-guard"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["enabled"], true);

    // Toggle guard off
    let req = Request::post(format!("/formations/{formation_id}/toggle-guard"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["enabled"], false);
}

#[tokio::test]
async fn test_cycle_intent_progression() {
    let (router, token) = build_test_app(true);

    // Create formation
    let body =
        serde_json::json!({ "name": "Intent Cycle", "intent": "Reconnoiter", "connectors": [] });
    let req = Request::post("/formations")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (_, json) = send(router.clone(), req).await;
    let id = json["id"].as_str().unwrap().to_owned();

    // Cycle: Reconnoiter → Execute
    let req = Request::post(format!("/formations/{id}/cycle-intent"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["intent"], "Execute");

    // Cycle: Execute → Stabilize
    let req = Request::post(format!("/formations/{id}/cycle-intent"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (_, json) = send(router, req).await;
    assert_eq!(json["intent"], "Stabilize");
}

#[tokio::test]
async fn test_cycle_autonomy() {
    let (router, token) = build_test_app(true);

    // Create formation
    let body = serde_json::json!({ "name": "Auto Cycle", "intent": "Execute", "connectors": [] });
    let req = Request::post("/formations")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (_, json) = send(router.clone(), req).await;
    let id = json["id"].as_str().unwrap().to_owned();

    // Cycle autonomy: suggest → act-with-approval
    let req = Request::post(format!("/formations/{id}/cycle-autonomy"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["level"].is_string());
}

/// §5.5 formation self-governance routes — propose-intent enqueues the
/// Fever-gated consensus command; cast_vote validates ids before
/// enqueueing a ballot.
#[tokio::test]
async fn test_propose_intent_and_cast_vote_routes() {
    let (router, token) = build_test_app(true);

    // Create a formation to address.
    let body = serde_json::json!({
        "name": "Governance Squad",
        "intent": "Reconnoiter",
        "guard_mode": false,
        "agents": [{
            "connector_name": "connector-test",
            "trigger_name": "event_received",
            "action_connector": "connector-test",
            "action_name": "send_message"
        }]
    });
    let req = Request::post("/formations/deploy-team")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::CREATED);
    let fid = json["formation_id"].as_str().unwrap().to_owned();

    // Propose an intent change — the route enqueues the consensus
    // command (the Fever gate is enforced by the bot event loop).
    let req = Request::post(format!("/formations/{fid}/propose-intent"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "intent": "Execute" })).unwrap(),
        ))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["proposed"], fid);

    // Missing intent body → 400.
    let req = Request::post(format!("/formations/{fid}/propose-intent"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(b"{}".to_vec()))
        .unwrap();
    let (status, _) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Cast a ballot with well-formed ids → enqueued (200).
    let vote_id = uuid::Uuid::new_v4();
    let voter = uuid::Uuid::new_v4();
    let req = Request::post(format!("/formations/{fid}/votes/{vote_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "voter": voter, "approve": true })).unwrap(),
        ))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["voted"], vote_id.to_string());

    // Malformed voter uuid → 400 from the runtime op's validation.
    let req = Request::post(format!("/formations/{fid}/votes/{vote_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "voter": "not-a-uuid", "approve": true }))
                .unwrap(),
        ))
        .unwrap();
    let (status, _) = send(router, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ────────────────────────────────────────────────────────────────────────────────
// SSE stream auth — one-time ticket, never a bearer token in the URL (plan 0.7)
// ────────────────────────────────────────────────────────────────────────────────

/// Helper: status of a streaming route. Does not read the body — an SSE
/// body never ends (keep-alive), so only the response head is inspected.
async fn stream_status(router: Router, request: Request<Body>) -> StatusCode {
    router.oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn test_stream_bearer_in_query_returns_401() {
    let (router, token) = build_test_app(true);

    // The old `?token=` fallback is gone: a valid bearer token in the query
    // string is not accepted on either the stream or an ordinary route.
    let req = Request::get(format!("/stream?token={token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        stream_status(router.clone(), req).await,
        StatusCode::UNAUTHORIZED
    );

    let req = Request::get(format!("/connectors?token={token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(router, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_stream_ticket_requires_bearer() {
    let (router, _token) = build_test_app(true);
    let req = Request::post("/stream/ticket").body(Body::empty()).unwrap();
    let (status, _) = send(router, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_stream_ticket_is_single_use() {
    let (router, token) = build_test_app(true);

    let req = Request::post("/stream/ticket")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    let ticket = body["ticket"].as_str().unwrap().to_owned();
    assert_eq!(ticket.len(), 64, "32 random bytes, hex-encoded");
    assert_eq!(body["ttl_secs"], 30);

    // Fresh ticket opens the stream.
    let req = Request::get(format!("/stream?ticket={ticket}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(stream_status(router.clone(), req).await, StatusCode::OK);

    // Same ticket again is rejected.
    let req = Request::get(format!("/stream?ticket={ticket}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        stream_status(router.clone(), req).await,
        StatusCode::UNAUTHORIZED
    );

    // Bearer header alone does not open a stream route; a ticket does.
    let req = Request::get("/chat/stream")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        stream_status(router.clone(), req).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn test_list_executions_returns_list() {
    let (router, token) = build_test_app(true);
    let req = Request::get("/executions")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn test_list_workspaces_returns_ok() {
    let (router, token) = build_test_app(true);
    let req = Request::get("/workspaces?formation_id=test-formation")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}
