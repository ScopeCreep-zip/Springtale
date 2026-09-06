//! API integration tests for the springtaled management API.
//!
//! These test the full HTTP layer (router + middleware + handlers) using
//! `tower::ServiceExt::oneshot()` — no TCP socket required.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use springtaled::test_harness::TestApp;

/// Build a test router with in-memory state. Returns (Router, hex-encoded token).
///
/// The construction lives in `springtaled::test_harness` so the CLI suite
/// can boot the same state over a real socket — one copy, not two.
///
/// The `ready` flag defaults to `true`. Pass `false` for tests that need
/// the daemon to appear as still booting.
fn build_test_app(ready: bool) -> (Router, String) {
    let app = TestApp::build(ready);
    (app.router, app.token_hex)
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

// ────────────────────────────────────────────────────────────────────────────────
// Data export / import / purge
// ────────────────────────────────────────────────────────────────────────────────

/// The rule body used by the data round-trip tests.
fn sample_rule(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "description": "data round-trip fixture",
        "status": "enabled",
        "version": 1,
        "trigger": { "type": "Webhook", "path": "my-hook" },
        "conditions": [],
        "actions": [ { "type": "SendMessage", "text": "hello" } ]
    })
}

/// The stored data as `POST /data/export` sees it.
///
/// Deliberately *not* `GET /rules`: that answers from the running rule
/// engine, so it is not proof that a row left the store. The export is
/// the store's own view, which is what purge and import act on.
async fn export_snapshot(router: &Router, token: &str) -> String {
    let req = Request::post("/data/export")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    body.to_string()
}

/// `POST /data/import` restores exactly what `POST /data/export` produced.
#[tokio::test]
async fn test_data_export_import_round_trips() {
    let (router, token) = build_test_app(true);

    let create = Request::post("/rules")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&sample_rule("round-trip-rule")).unwrap(),
        ))
        .unwrap();
    let (status, _) = send(router.clone(), create).await;
    assert_eq!(status, StatusCode::CREATED);

    // Export the snapshot.
    let req = Request::post("/data/export")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, snapshot) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        snapshot.to_string().contains("round-trip-rule"),
        "export did not capture the rule: {snapshot}"
    );

    // Wipe, so the import has something to restore.
    let req = Request::post("/data/purge")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"confirm":true}"#))
        .unwrap();
    let (status, body) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["purged"], true);
    assert!(
        !export_snapshot(&router, &token)
            .await
            .contains("round-trip-rule"),
        "purge left the rule behind"
    );

    // Import the snapshot back.
    let req = Request::post("/data/import")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&snapshot).unwrap()))
        .unwrap();
    let (status, _stats) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        export_snapshot(&router, &token)
            .await
            .contains("round-trip-rule"),
        "import did not restore the exported rule"
    );
}

/// Purge is destructive, so the confirmation is part of the wire format:
/// no body and `confirm: false` are both refused.
#[tokio::test]
async fn test_data_purge_requires_confirmation() {
    let (router, token) = build_test_app(true);

    let create = Request::post("/rules")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&sample_rule("survives-refused-purge")).unwrap(),
        ))
        .unwrap();
    let (status, _) = send(router.clone(), create).await;
    assert_eq!(status, StatusCode::CREATED);

    // No body at all — the extractor refuses before the handler runs.
    let req = Request::post("/data/purge")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(router.clone(), req).await;
    assert!(
        status.is_client_error(),
        "purge with no confirmation should be refused, got {status}"
    );

    // Explicit `false` — the handler's own 400.
    let req = Request::post("/data/purge")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"confirm":false}"#))
        .unwrap();
    let (status, _) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert!(
        export_snapshot(&router, &token)
            .await
            .contains("survives-refused-purge"),
        "a refused purge must not delete anything"
    );

    // With the confirmation it goes through.
    let req = Request::post("/data/purge")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"confirm":true}"#))
        .unwrap();
    let (status, body) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["purged"], true);
    assert!(
        !export_snapshot(&router, &token)
            .await
            .contains("survives-refused-purge"),
        "confirmed purge should empty the store"
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

// ────────────────────────────────────────────────────────────────────────────────
// MCP endpoint (plan 6.5)
// ────────────────────────────────────────────────────────────────────────────────

/// MCP transports spec: "Servers MUST validate the `Origin` header on all
/// incoming connections to prevent DNS rebinding attacks." A page on a
/// remote origin must not be able to drive the local daemon's MCP
/// endpoint, even if it somehow holds a token.
#[tokio::test]
async fn test_mcp_rejects_non_loopback_origin() {
    let (router, token) = build_test_app(true);
    let req = Request::post("/mcp")
        .header("authorization", format!("Bearer {token}"))
        .header("origin", "https://evil.example.com")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        ))
        .unwrap();
    let (status, _body) = send(router, req).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// The Origin check is not the authentication check: a loopback origin
/// with no bearer token is still rejected, and the `Mcp-Session-Id`
/// header never substitutes for one ("MCP Servers MUST NOT use sessions
/// for authentication").
#[tokio::test]
async fn test_mcp_requires_bearer_even_from_loopback() {
    let (router, _token) = build_test_app(true);
    let req = Request::post("/mcp")
        .header("origin", "http://127.0.0.1:9000")
        .header("mcp-session-id", "pretend-this-is-auth")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        ))
        .unwrap();
    let (status, _body) = send(router, req).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Plan 6.3 — bot persona / context window / tool policy are settings:
/// a PUT reaches the store and the following GET reads it back, with no
/// TOML edit and no restart in between.
#[tokio::test]
async fn test_bot_settings_put_then_get_round_trips() {
    let (router, token) = build_test_app(true);

    let body = serde_json::json!({
        "persona": { "name": "Mothra", "tone": "warm", "prefix": "!" },
        "context_window": 12,
        // Glob patterns describe connectors that need not be installed,
        // so this stays independent of the fixture's registry contents.
        "tool_policy": { "allow": ["connector-telegram__*"], "deny": [] },
    });
    let req = Request::put("/bot/settings")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, saved) = send(router.clone(), req).await;
    assert_eq!(status, StatusCode::OK, "PUT rejected: {saved}");
    assert_eq!(saved["saved"], true);

    let req = Request::get("/bot/settings")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, got) = send(router, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["persona"]["name"], "Mothra");
    assert_eq!(got["persona"]["prefix"], "!");
    assert_eq!(got["context_window"], 12);
    assert_eq!(got["tool_policy"]["allow"][0], "connector-telegram__*");
}

/// A literal (non-glob) tool that names no installed action is refused at
/// the boundary — a typo must not silently leave the AI tool-less.
#[tokio::test]
async fn test_bot_settings_rejects_unknown_tool() {
    let (router, token) = build_test_app(true);

    let body = serde_json::json!({
        "persona": { "name": "Springtale", "tone": "neutral", "prefix": "/" },
        "context_window": 50,
        "tool_policy": { "allow": ["connector-nope__do_thing"], "deny": [] },
    });
    let req = Request::put("/bot/settings")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, body) = send(router, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown tool"),
        "expected unknown-tool error, got {body}"
    );
}
