//! API integration tests for the springtaled management API.
//!
//! These test the full HTTP layer (router + middleware + handlers) using
//! `tower::ServiceExt::oneshot()` — no TCP socket required.

#![allow(clippy::unwrap_used, clippy::expect_used)]

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
    let trigger_registry = springtaled::runtime::boot::connector_events::TriggerRegistry::new(
        trigger_tx.clone(),
    );

    let heartbeat_monitor = std::sync::Arc::new(tokio::sync::Mutex::new(
        springtale_scheduler::HeartbeatMonitor::new(0, trigger_tx.clone()),
    ));

    let canvas = std::sync::Arc::new(tokio::sync::RwLock::new(
        springtale_core::canvas::CanvasState::default(),
    ));
    let (canvas_tx, _rx) = tokio::sync::broadcast::channel(64);

    let wasm_engine = std::sync::Arc::new(
        springtale_connector::wasm::WasmEngine::new(
            springtale_connector::wasm::SandboxLimits::default(),
        )
        .expect("WASM engine creation"),
    );

    let runtime = springtale_runtime::RuntimeState {
        store,
        registry,
        engine,
        ai_adapter,
        sentinel,
        wasm_engine,
        canvas,
        canvas_tx,
    };

    let state = AppState {
        runtime,
        api_token_hash,
        ready: ready_flag,
        trigger_tx,
        scheduler: springtaled::scheduler::AppScheduler { cron, fs_watcher },
        rate_limit_per_sec: 1000,
        event_tx,
        heartbeat_monitor,
        bot_msg_tx,
        trigger_registry,
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

    // Step up from default "suggest" to "act-with-approval"
    let body = serde_json::json!({ "direction": "up" });
    let req = Request::post("/agents/test-agent/autonomy/step")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["level"], "act-with-approval");

    // Step down back to "suggest"
    let body = serde_json::json!({ "direction": "down" });
    let req = Request::post("/agents/test-agent/autonomy/step")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, json) = send(router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["level"], "suggest");
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
