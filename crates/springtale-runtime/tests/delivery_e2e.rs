//! End-to-end delivery proof: a fired `Notify` / `SendMessage` chain
//! actually reaches the user.
//!
//! Before the delivery layer, `Action::Notify` only wrote a
//! `tracing::info!` line and `Action::SendMessage` logged "no
//! destination context" — so a scheduled weather briefing, hydration
//! reminder, cron-runner, etc. fired correctly and the user received
//! NOTHING. Parse/skeleton tests never caught it because they never
//! ran the chain through the job consumer.
//!
//! This boots the REAL embedded runtime (in-memory store, NoopAdapter
//! — no AI), enqueues a fired rule chain through the SAME `JobProducer`
//! the cron/trigger loops use, and asserts the user-facing delivery
//! arrives on `notification_tx`. It exercises the whole delivery path:
//! producer → job consumer → `dispatch_actions` →
//! `NotificationEvent::from_chain` → broadcast.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use springtale_core::rule::action::Action;
use springtale_runtime::{EmbeddedScheduler, RuntimeConfig, RuntimeState, StoreConfig};

/// Boot a minimal embedded runtime over an ephemeral in-memory store
/// with the default NoopAdapter (no AI configured).
async fn boot() -> (RuntimeState, EmbeddedScheduler) {
    let config = RuntimeConfig {
        store: StoreConfig {
            ephemeral: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (formation_cmd_tx, _formation_cmd_rx) = tokio::sync::mpsc::channel(16);
    let state = springtale_runtime::init(&config, formation_cmd_tx, None, None)
        .await
        .expect("runtime init");
    let handle = springtale_runtime::bootstrap_embedded(&state, 0)
        .await
        .expect("embedded bootstrap");
    (state, handle.scheduler)
}

/// A fired rule chain payload, in the shape the embedded job consumer
/// deserializes (`ChainJob`). Built as JSON so the test doesn't need
/// the private struct.
fn chain_job(actions: Vec<Action>) -> serde_json::Value {
    serde_json::json!({
        "rule_id": null,
        "trigger_type": "Cron",
        "trigger_payload": {},
        "actions": actions,
    })
}

#[tokio::test]
async fn fired_notify_reaches_the_user() {
    let (state, scheduler) = boot().await;
    let mut rx = state.notification_tx.subscribe();

    let payload = chain_job(vec![Action::Notify {
        title: "Weather".to_owned(),
        body: "It's 72°F in Sacramento, CA.".to_owned(),
    }]);
    scheduler
        .producer
        .enqueue(payload, 3)
        .await
        .expect("enqueue");

    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("delivery within 5s — Notify must reach the user")
        .expect("notification channel open");
    assert_eq!(event.title, "Weather");
    assert_eq!(event.body, "It's 72°F in Sacramento, CA.");
}

#[tokio::test]
async fn fired_send_message_reaches_the_user() {
    let (state, scheduler) = boot().await;
    let mut rx = state.notification_tx.subscribe();

    let payload = chain_job(vec![Action::SendMessage {
        text: "PR #42 merged".to_owned(),
    }]);
    scheduler
        .producer
        .enqueue(payload, 3)
        .await
        .expect("enqueue");

    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("delivery within 5s — SendMessage must reach the user")
        .expect("notification channel open");
    // A destination-less SendMessage routes to the in-app chat as a
    // generic "Message".
    assert_eq!(event.title, "Message");
    assert_eq!(event.body, "PR #42 merged");
}

#[tokio::test]
async fn fired_chain_resolves_templates_before_delivery() {
    let (state, scheduler) = boot().await;
    let mut rx = state.notification_tx.subscribe();

    // A Notify whose body references the trigger payload — the user
    // must see the resolved value, never the raw `${...}` placeholder.
    let payload = serde_json::json!({
        "rule_id": null,
        "trigger_type": "Cron",
        "trigger_payload": { "city": "Tucson" },
        "actions": [{
            "type": "Notify",
            "title": "Reminder",
            "body": "Good morning, ${trigger.city}!",
        }],
    });
    scheduler
        .producer
        .enqueue(payload, 3)
        .await
        .expect("enqueue");

    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("delivery within 5s")
        .expect("notification channel open");
    assert_eq!(event.body, "Good morning, Tucson!");
    assert!(
        !event.body.contains("${"),
        "placeholder leaked: {}",
        event.body
    );
}
