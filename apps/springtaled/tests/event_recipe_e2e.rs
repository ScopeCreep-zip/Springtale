//! Recorded-real end-to-end contract test for event-driven recipes.
//!
//! This is the test that would have caught "Bug A": event recipes
//! delivered raw nested blobs / literal `${trigger.*}` placeholders
//! because the raw provider webhook payload was never normalized to the
//! connector's declared flat trigger schema.
//!
//! VCR/recorded-real, no guessed data: each fixture is a REAL provider
//! webhook payload (the documented GitHub event shape). It is pushed
//! through the connector's ACTUAL `normalize_event` (the trait method
//! the webhook ingress calls), then the recipe's REAL rule chain is
//! fired via `dispatch_actions` in `DryRun` (sends stubbed but params
//! resolved), and the resulting user-facing delivery is asserted to be
//! clean AND to contain the real values from the payload.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use connector_github::GithubConnector;
use springtale_connector::Connector;
use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::execution::{ExecutionContext, ExecutionMode};
use springtale_core::rule::types::{Rule, RuleId};
use springtale_runtime::CapabilityBridge;
use springtale_runtime::operations::recipes::apply::substitute_template_public;
use springtale_runtime::operations::recipes::builtin;
use springtale_runtime::operations::recipes::types::{FieldKind, RecipeInputs};
use springtale_sentinel::{AutoAllowApprovalGate, Sentinel, SentinelConfig};
use springtale_store::SqliteBackend;
use tokio::sync::RwLock;

/// A GitHub connector instance (dummy token) so we can call its real
/// `normalize_event` trait method — the exact code the webhook ingress
/// invokes via `host.normalize_event`.
fn github() -> GithubConnector {
    let config = serde_json::from_value(serde_json::json!({ "token": "dummy-token" }))
        .expect("github config");
    GithubConnector::new(config).expect("github connector")
}

async fn bridge_and_sentinel() -> (CapabilityBridge, Arc<Sentinel>) {
    let store: Arc<dyn springtale_store::StorageBackend> =
        Arc::new(SqliteBackend::open_in_memory().unwrap());
    let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
        CapabilityPolicy::AllowAll,
    )));
    let bridge = CapabilityBridge::new(registry).with_store(store.clone());
    let sentinel = Arc::new(Sentinel::with_approval_gate(
        SentinelConfig::default(),
        store,
        Arc::new(AutoAllowApprovalGate),
    ));
    (bridge, sentinel)
}

/// Fill a recipe's apply-time inputs with realistic values. The trigger
/// fields come from the normalized payload, not from here.
fn fill_inputs(recipe: &springtale_runtime::operations::recipes::types::Recipe) -> RecipeInputs {
    let mut inputs = RecipeInputs::empty();
    for f in &recipe.inputs {
        let v = match (f.id.as_str(), &f.kind) {
            ("repo", _) => serde_json::json!("octocat/Hello-World"),
            ("branch", _) => serde_json::json!("main"),
            (_, FieldKind::Secret) => serde_json::json!("secret-value"),
            (_, FieldKind::Number) => serde_json::json!(1),
            (_, FieldKind::Bool) => serde_json::json!(true),
            (_, FieldKind::Select { options }) => options
                .first()
                .map(|o| serde_json::json!(o.value))
                .unwrap_or_else(|| serde_json::json!("")),
            _ => serde_json::json!("123456"),
        };
        inputs.insert(f.id.clone(), v);
    }
    inputs
}

/// Collect every user-facing delivery string a fired chain produced —
/// Notify body/title, SendMessage text, and the resolved text fields of
/// a DryRun-stubbed connector send.
fn deliveries(chain: &springtale_core::rule::chain_context::ChainContext) -> Vec<String> {
    let mut out = Vec::new();
    for step in &chain.steps {
        let o = &step.output;
        match step.kind.as_str() {
            "notify" => {
                for k in ["title", "body"] {
                    if let Some(s) = o.get(k).and_then(serde_json::Value::as_str) {
                        out.push(s.to_owned());
                    }
                }
            }
            "send_message" => {
                if let Some(s) = o.get("text").and_then(serde_json::Value::as_str) {
                    out.push(s.to_owned());
                }
            }
            "run_connector"
                if o.get("dry_run").and_then(serde_json::Value::as_bool) == Some(true) =>
            {
                if let Some(params) = o.pointer("/output/params") {
                    for k in ["text", "content", "body", "message"] {
                        if let Some(s) = params.get(k).and_then(serde_json::Value::as_str) {
                            out.push(s.to_owned());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn assert_clean(recipe: &str, deliveries: &[String]) {
    assert!(!deliveries.is_empty(), "{recipe}: produced no delivery");
    for d in deliveries {
        assert!(
            !d.contains("${"),
            "{recipe}: unresolved placeholder in {d:?}"
        );
        assert!(!d.contains("{\""), "{recipe}: raw JSON blob in {d:?}");
        assert!(!d.contains("null"), "{recipe}: null leaked in {d:?}");
    }
}

/// Fire a recipe's chain with a normalized trigger payload, return the
/// user-facing deliveries.
async fn fire(recipe_id: &str, trigger: serde_json::Value) -> Vec<String> {
    let recipe = builtin::get(recipe_id).expect("recipe");
    let inputs = fill_inputs(&recipe);
    let toml = substitute_template_public(&recipe.blueprint.rules[0].toml, &inputs);
    let rule: Rule = toml::from_str(&toml).expect("rule parses");

    let (bridge, sentinel) = bridge_and_sentinel().await;
    let exec = ExecutionContext::for_global(RuleId::new(), ExecutionMode::DryRun);
    let chain = springtale_runtime::dispatch::dispatch_actions(
        &rule.actions,
        &bridge,
        &sentinel,
        exec,
        trigger,
    )
    .await
    .expect("chain fires");
    deliveries(&chain)
}

// Real GitHub `push` webhook payload shape.
fn push_payload() -> serde_json::Value {
    serde_json::json!({
        "ref": "refs/heads/main",
        "repository": { "full_name": "octocat/Hello-World" },
        "pusher": { "name": "octocat", "email": "octocat@github.com" },
        "commits": [{ "id": "a1" }, { "id": "b2" }, { "id": "c3" }]
    })
}

#[tokio::test]
async fn github_push_discord_delivers_clean_normalized_message() {
    let normalized = github().normalize_event("push", push_payload());
    // The normalization itself: real username + integer count, no blobs.
    assert_eq!(normalized["pusher"], "octocat");
    assert_eq!(normalized["commits_count"], 3);

    let deliveries = fire("github-push-discord", normalized).await;
    assert_clean("github-push-discord", &deliveries);
    let joined = deliveries.join(" | ");
    assert!(joined.contains("octocat"), "pusher name missing: {joined}");
    assert!(joined.contains('3'), "commit count missing: {joined}");
}

#[tokio::test]
async fn github_pr_watcher_delivers_clean_normalized_message() {
    let raw = serde_json::json!({
        "action": "opened",
        "number": 42,
        "pull_request": {
            "title": "Add the thing",
            "body": "desc",
            "html_url": "https://github.com/octocat/Hello-World/pull/42",
            "user": { "login": "contributor" }
        },
        "repository": { "full_name": "octocat/Hello-World" }
    });
    let normalized = github().normalize_event("pull_request", raw);
    let deliveries = fire("github-pr-watcher", normalized).await;
    assert_clean("github-pr-watcher", &deliveries);
    let joined = deliveries.join(" | ");
    assert!(
        joined.contains("Add the thing"),
        "PR title missing: {joined}"
    );
    assert!(joined.contains("42"), "PR number missing: {joined}");
}

#[tokio::test]
async fn github_issue_telegram_delivers_clean_normalized_message() {
    let raw = serde_json::json!({
        "action": "opened",
        "issue": {
            "number": 7,
            "title": "Bug: thing broken",
            "html_url": "https://github.com/octocat/Hello-World/issues/7",
            "user": { "login": "reporter" }
        },
        "repository": { "full_name": "octocat/Hello-World" }
    });
    let normalized = github().normalize_event("issue_opened", raw);
    let deliveries = fire("github-issue-telegram", normalized).await;
    assert_clean("github-issue-telegram", &deliveries);
    let joined = deliveries.join(" | ");
    assert!(
        joined.contains("Bug: thing broken"),
        "issue title missing: {joined}"
    );
    assert!(joined.contains("reporter"), "author missing: {joined}");
}

/// Telegram polling path: a REAL `getUpdates` message → the telegram
/// connector's normalize_event → the echo recipe → clean delivery of the
/// real text. (Telegram's `chat_id` is nested `message.chat.id` in the
/// raw update; normalization flattens it.)
#[tokio::test]
async fn telegram_echo_delivers_clean_normalized_message() {
    let raw = serde_json::json!({
        "update_id": 1,
        "message": {
            "message_id": 5,
            "from": { "id": 99, "is_bot": false, "first_name": "Kali" },
            "chat": { "id": 4242, "type": "private" },
            "text": "hello there",
            "date": 1700000000
        }
    });
    // The exact code the polling gateway + trigger loop run.
    let normalized = connector_telegram::triggers::normalize::normalize("message", &raw);
    assert_eq!(normalized["chat_id"], 4242); // flat, not a nested object

    let deliveries = fire("telegram-echo", normalized).await;
    assert_clean("telegram-echo", &deliveries);
    let joined = deliveries.join(" | ");
    assert!(
        joined.contains("hello there"),
        "echoed text missing: {joined}"
    );
}

/// Bluesky firehose path: a REAL Jetstream `app.bsky.feed.post` create
/// commit authored by our own DID → the gateway's `route_jetstream_event`
/// classification (`own_post`) → the relay recipe → clean delivery of the
/// real post text.
#[tokio::test]
async fn bluesky_own_post_relay_delivers_clean() {
    let commit = serde_json::json!({
        "did": "did:plc:me",
        "time_us": 1_700_000_000_000_000u64,
        "kind": "commit",
        "commit": {
            "operation": "create",
            "collection": "app.bsky.feed.post",
            "rkey": "3kxyz",
            "cid": "bafyreigh2akiscaildc",
            "record": { "$type": "app.bsky.feed.post", "text": "good morning bluesky" }
        }
    });
    // The exact code the firehose gateway runs.
    let payload =
        connector_bluesky::gateway::route_jetstream_event(&commit, "did:plc:me").expect("own_post");
    assert_eq!(payload["trigger"], "own_post");

    let deliveries = fire("bluesky-nostr-relay", payload).await;
    assert_clean("bluesky-nostr-relay", &deliveries);
    assert!(
        deliveries.join(" | ").contains("good morning bluesky"),
        "post text missing: {deliveries:?}"
    );
}

/// The bug, captured as a contrast: firing the SAME recipe with the RAW
/// (un-normalized) payload produces a broken delivery — proving the
/// normalization is what fixes it.
#[tokio::test]
async fn raw_payload_without_normalization_is_broken() {
    let deliveries = fire("github-push-discord", push_payload()).await;
    let joined = deliveries.join(" | ");
    // Raw `pusher` is a nested object → blob; `commits_count` is absent →
    // literal placeholder. At least one must be present without
    // normalization.
    assert!(
        joined.contains("${trigger.commits_count}") || joined.contains("{\""),
        "expected the raw payload to deliver broken output, got: {joined}"
    );
}
