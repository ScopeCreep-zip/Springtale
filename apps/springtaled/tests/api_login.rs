//! Login and issued-token tests (plan 6.6, finding 109).
//!
//! The invariant under test: a bearer is only ever something the daemon
//! *issued*. Nothing derived from the vault passphrase authenticates.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use springtaled::test_harness::TestApp;

/// The passphrase `TestApp` builds its verifier from.
const PASSPHRASE: &str = "test-passphrase";

async fn send(router: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

fn get_with(path: &str, token: &str) -> Request<Body> {
    Request::get(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn post_json(path: &str, token: Option<&str>, body: serde_json::Value) -> Request<Body> {
    let mut req = Request::post(path).header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn test_login_with_correct_passphrase_issues_a_working_token() {
    let app = TestApp::build(true);
    let (status, body) = send(
        app.router.clone(),
        post_json(
            "/auth/login",
            None,
            serde_json::json!({ "passphrase": PASSPHRASE }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["token"].as_str().expect("token in body").to_owned();
    // 32 bytes of entropy, hex — OWASP asks for at least 64 bits.
    assert_eq!(token.len(), 64);
    assert!(body["expires_in"].as_u64().unwrap() > 0);

    let (status, _) = send(app.router.clone(), get_with("/connectors", &token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_login_with_wrong_passphrase_is_refused() {
    let app = TestApp::build(true);
    let (status, body) = send(
        app.router.clone(),
        post_json(
            "/auth/login",
            None,
            serde_json::json!({ "passphrase": "not-the-passphrase" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body["token"].is_null());
}

#[tokio::test]
async fn test_derived_passphrase_hash_is_not_a_bearer() {
    let app = TestApp::build(true);
    // Exactly the value the pre-6.6 client computed and sent.
    let derived = hex::encode(springtale_crypto::token::derive_api_token_hash(
        PASSPHRASE.as_bytes(),
    ));
    let (status, _) = send(app.router.clone(), get_with("/connectors", &derived)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_token_that_was_never_issued_is_refused() {
    let app = TestApp::build(true);
    let never_issued = hex::encode([0x5au8; 32]);
    let (status, _) = send(app.router.clone(), get_with("/connectors", &never_issued)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Not hex, and hex of the wrong width, are refused too.
    for bad in ["not-hex-at-all", "abcd"] {
        let (status, _) = send(app.router.clone(), get_with("/connectors", bad)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn test_session_expires_after_its_idle_timeout() {
    let app = TestApp::build(true);
    let (status, body) = send(
        app.router.clone(),
        post_json(
            "/auth/login",
            None,
            serde_json::json!({ "passphrase": PASSPHRASE }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["token"].as_str().unwrap().to_owned();

    // Age every live session past the 30-minute idle window without
    // sleeping through it.
    {
        let mut sessions = app.state.sessions.lock().await;
        let stale = Instant::now() - Duration::from_secs(1_801);
        for rec in sessions.values_mut() {
            rec.last_seen = stale;
        }
    }

    let (status, _) = send(app.router.clone(), get_with("/connectors", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_logout_drops_the_session_immediately() {
    let app = TestApp::build(true);
    let (_, body) = send(
        app.router.clone(),
        post_json(
            "/auth/login",
            None,
            serde_json::json!({ "passphrase": PASSPHRASE }),
        ),
    )
    .await;
    let token = body["token"].as_str().unwrap().to_owned();

    let (status, body) = send(
        app.router.clone(),
        post_json("/auth/logout", Some(&token), serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["logged_out"], serde_json::json!(true));

    let (status, _) = send(app.router.clone(), get_with("/connectors", &token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_revoked_long_lived_token_stops_working_immediately() {
    let app = TestApp::build(true);
    let session = app.token_hex.clone();

    let (status, body) = send(
        app.router.clone(),
        post_json(
            "/auth/tokens",
            Some(&session),
            serde_json::json!({ "name": "springtale-cli@test" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = body["id"].as_str().unwrap().to_owned();
    let long_lived = body["token"].as_str().unwrap().to_owned();
    assert_eq!(long_lived.len(), 64);
    assert_ne!(long_lived, session);

    let (status, _) = send(app.router.clone(), get_with("/connectors", &long_lived)).await;
    assert_eq!(status, StatusCode::OK);

    // Listing never leaks the hash, let alone the token.
    let (status, body) = send(app.router.clone(), get_with("/auth/tokens", &session)).await;
    assert_eq!(status, StatusCode::OK);
    let listed = body["tokens"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].get("token").is_none());
    assert!(listed[0].get("token_hash").is_none());

    let revoke = Request::delete(format!("/auth/tokens/{id}"))
        .header("authorization", format!("Bearer {session}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app.router.clone(), revoke).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send(app.router.clone(), get_with("/connectors", &long_lived)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_stream_ticket_dies_with_its_session() {
    let app = TestApp::build(true);
    let (_, body) = send(
        app.router.clone(),
        post_json(
            "/auth/login",
            None,
            serde_json::json!({ "passphrase": PASSPHRASE }),
        ),
    )
    .await;
    let token = body["token"].as_str().unwrap().to_owned();

    let (status, body) = send(
        app.router.clone(),
        post_json("/stream/ticket", Some(&token), serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = body["ticket"].as_str().unwrap().to_owned();

    // Log out, then try to cash the ticket in.
    let (status, _) = send(
        app.router.clone(),
        post_json("/auth/logout", Some(&token), serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let req = Request::get(format!("/stream?ticket={ticket}"))
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
