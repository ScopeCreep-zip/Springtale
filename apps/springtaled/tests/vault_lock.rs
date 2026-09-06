//! The daemon locks — plan 6.10, finding 113.
//!
//! The point of the feature is that `springtaled` keeps *running* with
//! the key gone, so every test here drives the outer router through a
//! full lock → refuse → unlock → serve cycle without the process (or
//! the fixture) restarting.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use springtaled::test_harness::{TEST_PASSPHRASE, TestGuard};

/// Send a request and read the JSON body back.
async fn send(router: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// `GET /connectors` with a bearer token — a representative
/// authenticated route.
fn authed_get(token_hex: &str) -> Request<Body> {
    Request::builder()
        .uri("/connectors")
        .header("authorization", format!("Bearer {token_hex}"))
        .body(Body::empty())
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

/// `POST /vault/unlock` with the given passphrase.
fn unlock_request(passphrase: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/vault/unlock")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "passphrase": passphrase }).to_string(),
        ))
        .unwrap()
}

/// `POST /vault/lock`, optionally authenticated.
fn lock_request(token_hex: Option<&str>) -> Request<Body> {
    let builder = Request::builder().method("POST").uri("/vault/lock");
    let builder = match token_hex {
        Some(t) => builder.header("authorization", format!("Bearer {t}")),
        None => builder,
    };
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn test_locked_daemon_refuses_authenticated_route_with_503() {
    let app = TestGuard::build();

    // Unlocked, the route works.
    let (status, _) = send(app.router.clone(), authed_get(&app.token_hex)).await;
    assert_eq!(status, StatusCode::OK);

    assert!(app.guard.lock().await, "lock must take effect");
    assert!(app.guard.is_locked());

    // A valid bearer token buys nothing: the runtime that would have
    // answered no longer exists.
    let (status, body) = send(app.router.clone(), authed_get(&app.token_hex)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["locked"], serde_json::json!(true));
}

#[tokio::test]
async fn test_ready_reports_locked_state() {
    let app = TestGuard::build();

    let (status, body) = send(app.router.clone(), get("/ready")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["locked"], serde_json::json!(false));
    assert_eq!(body["status"], serde_json::json!("ready"));

    app.guard.lock().await;

    let (status, body) = send(app.router.clone(), get("/ready")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a locked daemon is still a live daemon"
    );
    assert_eq!(body["locked"], serde_json::json!(true));
    assert_eq!(body["status"], serde_json::json!("locked"));
}

#[tokio::test]
async fn test_health_answers_while_locked() {
    let app = TestGuard::build();
    app.guard.lock().await;

    let (status, body) = send(app.router.clone(), get("/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], serde_json::json!("ok"));
}

#[tokio::test]
async fn test_unlock_restores_service() {
    let app = TestGuard::build();
    app.guard.lock().await;
    assert_eq!(
        send(app.router.clone(), authed_get(&app.token_hex)).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );

    let (status, body) = send(app.router.clone(), unlock_request(TEST_PASSPHRASE)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["locked"], serde_json::json!(false));
    assert!(!app.guard.is_locked());

    // The rebuilt runtime serves the same authenticated route, and
    // `/ready` reports a live store again.
    let (status, _) = send(app.router.clone(), authed_get(&app.token_hex)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send(app.router.clone(), get("/ready")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["locked"], serde_json::json!(false));
}

#[tokio::test]
async fn test_unlock_with_wrong_passphrase_stays_locked() {
    let app = TestGuard::build();
    app.guard.lock().await;

    let (status, body) = send(app.router.clone(), unlock_request("not-the-passphrase")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["locked"], serde_json::json!(true));
    // No oracle: the refusal says nothing about why.
    assert_eq!(body["error"], serde_json::json!("unlock failed"));
    assert!(app.guard.is_locked());

    assert_eq!(
        send(app.router.clone(), authed_get(&app.token_hex)).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn test_unlock_on_an_unlocked_daemon_is_a_conflict() {
    let app = TestGuard::build();
    let (status, _) = send(app.router.clone(), unlock_request(TEST_PASSPHRASE)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!app.guard.is_locked());
}

#[tokio::test]
async fn test_lock_route_requires_authentication() {
    let app = TestGuard::build();

    let (status, _) = send(app.router.clone(), lock_request(None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        !app.guard.is_locked(),
        "an unauthenticated POST must not lock"
    );

    let (status, _) = send(
        app.router.clone(),
        lock_request(Some("00".repeat(32).as_str())),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!app.guard.is_locked());
}

#[tokio::test]
async fn test_lock_route_locks_and_is_idempotent() {
    let app = TestGuard::build();

    let (status, body) = send(app.router.clone(), lock_request(Some(&app.token_hex))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["locked"], serde_json::json!(true));
    assert!(app.guard.is_locked());

    // Locking a locked daemon is a no-op, not an error — a panic button
    // must not have to know the current state.
    let (status, body) = send(app.router.clone(), lock_request(Some(&app.token_hex))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["locked"], serde_json::json!(true));
}

#[tokio::test]
async fn test_lock_then_unlock_cycles_repeatedly() {
    let app = TestGuard::build();
    for round in 0..3 {
        assert!(app.guard.lock().await, "round {round}: lock");
        assert_eq!(
            send(app.router.clone(), authed_get(&app.token_hex)).await.0,
            StatusCode::SERVICE_UNAVAILABLE,
            "round {round}: refused while locked"
        );
        let (status, _) = send(app.router.clone(), unlock_request(TEST_PASSPHRASE)).await;
        assert_eq!(status, StatusCode::OK, "round {round}: unlock");
        assert_eq!(
            send(app.router.clone(), authed_get(&app.token_hex)).await.0,
            StatusCode::OK,
            "round {round}: serving again"
        );
    }
}

/// The point of the whole design: locking is not a flag over a live
/// runtime, it drops the runtime. If any clone survived, the SQLite
/// handle would stay open and the database key would stay in memory.
#[tokio::test]
async fn test_lock_drops_the_store_handle() {
    let app = TestGuard::build();
    let store = {
        let live = app.guard.live().expect("unlocked");
        Arc::downgrade(&live.state().runtime.store)
    };
    assert!(store.upgrade().is_some(), "store is open while unlocked");

    app.guard.lock().await;

    assert!(
        store.upgrade().is_none(),
        "a locked daemon must hold no reference to the store"
    );
}
