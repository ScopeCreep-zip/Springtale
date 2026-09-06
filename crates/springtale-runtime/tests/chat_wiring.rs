//! Chat ingestion follows the registry, not the TOML (plan 6.4).
//!
//! A connector installed after boot must start receiving immediately,
//! and disabling it must stop its loop. Driven through a stub
//! `ChatSource` rather than a real connector: every first-party chat
//! connector needs live credentials and a reachable provider, which a
//! test can't supply.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use springtale_connector::chat::{ChatMessage, ChatSource, SharedChatSource};
use springtale_connector::connector::subscription::Subscription;
use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};
use springtale_runtime::operations::connectors::{disable_connector, wire_chat};
use springtale_runtime::{RuntimeConfig, RuntimeState, StoreConfig};
use tokio::sync::{mpsc, watch};

const NAME: &str = "connector-stub-chat";

/// Pushes a message every 20ms until shutdown flips, then records that
/// it stopped so the disable test can prove the loop actually ended.
struct StubChatSource {
    stopped: Arc<AtomicBool>,
}

#[async_trait]
impl ChatSource for StubChatSource {
    async fn run(
        &self,
        tx: mpsc::Sender<ChatMessage>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), ConnectorError> {
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                () = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                    let msg = ChatMessage::chat(
                        NAME,
                        "channel-1",
                        "user-1",
                        "hello",
                        serde_json::json!({ "trigger": "message" }),
                    );
                    if tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
        }
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn send(&self, _channel_id: &str, _text: &str) -> Result<(), ConnectorError> {
        Ok(())
    }
}

struct StubConnector {
    manifest: ConnectorManifest,
    chat: Arc<StubChatSource>,
}

#[async_trait]
impl Connector for StubConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        &self.manifest.triggers
    }

    fn actions(&self) -> &[ActionDecl] {
        &self.manifest.actions
    }

    async fn execute(
        &self,
        action: &str,
        _input: serde_json::Value,
    ) -> Result<ActionResult, ConnectorError> {
        Err(ConnectorError::NotFound(action.to_owned()))
    }

    async fn on_event(
        &self,
        trigger: &str,
        _handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        Err(ConnectorError::NotFound(trigger.to_owned()))
    }

    async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn verify_webhook(
        &self,
        _headers: &std::collections::HashMap<String, String>,
        _body: &[u8],
    ) -> Result<(), ConnectorError> {
        Ok(())
    }

    fn chat_source(&self) -> Option<SharedChatSource> {
        Some(self.chat.clone())
    }
}

async fn boot() -> RuntimeState {
    let config = RuntimeConfig {
        store: StoreConfig {
            ephemeral: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (formation_cmd_tx, _formation_cmd_rx) = tokio::sync::mpsc::channel(16);
    springtale_runtime::init(&config, formation_cmd_tx, None, None)
        .await
        .expect("runtime init")
}

fn stub(stopped: Arc<AtomicBool>) -> StubConnector {
    let manifest: ConnectorManifest = serde_json::from_value(serde_json::json!({
        "name": NAME,
        "version": "1.0.0",
        "author": "springtale-tests",
        "description": "stub chat connector",
        "capabilities": [],
    }))
    .expect("stub manifest parses");
    StubConnector {
        manifest,
        chat: Arc::new(StubChatSource { stopped }),
    }
}

/// A connector installed after boot is wired and its messages reach the
/// channel the bot event loop consumes.
#[tokio::test]
async fn connector_installed_at_runtime_reaches_the_bot_channel() {
    let state = boot().await;
    let mut chat_rx = state.take_chat_rx().await.expect("chat receiver");

    assert!(
        state.registry.read().await.get(NAME).is_none(),
        "stub connector must not be present at boot"
    );

    let stopped = Arc::new(AtomicBool::new(false));
    state
        .registry
        .write()
        .await
        .install_native(Box::new(stub(stopped)))
        .expect("stub installs");
    wire_chat(&state, NAME).await.expect("chat wires");

    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), chat_rx.recv())
        .await
        .expect("a message arrives within the timeout")
        .expect("chat channel stays open");
    assert_eq!(msg.connector, NAME);
    assert_eq!(msg.text, "hello");
}

/// Disabling the connector stops the loop: no further messages, and the
/// source itself observed the shutdown signal.
#[tokio::test]
async fn disabling_a_connector_stops_its_chat_loop() {
    let state = boot().await;
    let mut chat_rx = state.take_chat_rx().await.expect("chat receiver");

    let stopped = Arc::new(AtomicBool::new(false));
    state
        .registry
        .write()
        .await
        .install_native(Box::new(stub(stopped.clone())))
        .expect("stub installs");
    wire_chat(&state, NAME).await.expect("chat wires");

    tokio::time::timeout(std::time::Duration::from_secs(5), chat_rx.recv())
        .await
        .expect("the loop is running before disable")
        .expect("chat channel stays open");

    disable_connector(&state, NAME).await.expect("disable");

    // Drain whatever was already in flight, then prove the loop ended.
    for _ in 0..64 {
        if chat_rx.try_recv().is_err() {
            break;
        }
    }
    for _ in 0..50 {
        if stopped.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        stopped.load(Ordering::SeqCst),
        "the chat source observed the shutdown signal"
    );
    assert!(
        !state.chat_tasks.contains_key(NAME),
        "the wiring entry is dropped on disable"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), chat_rx.recv())
            .await
            .is_err(),
        "no further messages after disable"
    );
}
