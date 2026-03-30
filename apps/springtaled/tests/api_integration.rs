//! API integration tests for the springtaled management API.
//!
//! These test the full HTTP layer (router + middleware + handlers) using
//! `tower::ServiceExt::oneshot()` — no TCP socket required.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tokio::sync::{mpsc, Mutex, RwLock};
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
    let store = Arc::new(SqliteBackend::open_in_memory().unwrap());
    let registry = Arc::new(RwLock::new(ConnectorRegistry::new(CapabilityPolicy::AllowAll)));
    let engine = Arc::new(RwLock::new(RuleEngine::new()));

    let passphrase = b"test-passphrase";
    let api_token_hash = derive_api_token_hash(passphrase);
    let token_hex = hex::encode(api_token_hash);

    let (trigger_tx, _trigger_rx) = mpsc::channel(256);
    let cron = Arc::new(Mutex::new(CronExecutor::new(trigger_tx.clone())));
    let fs_watcher = Arc::new(Mutex::new(
        FsWatcher::new(trigger_tx.clone()).unwrap(),
    ));

    let ready_flag = Arc::new(AtomicBool::new(ready));

    let state = AppState {
        store,
        registry,
        engine,
        api_token_hash,
        ready: ready_flag,
        trigger_tx,
        cron,
        fs_watcher,
        rate_limit_per_sec: 1000,
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
    let json: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
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
