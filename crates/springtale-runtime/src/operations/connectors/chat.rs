//! Chat wiring — starts and stops a connector's [`ChatSource`] loop.
//!
//! Chat ingestion follows the registry, not the TOML. Every install
//! path (headless TOML at boot, `setup_connector` from the UI, enable,
//! reload) ends in [`wire_chat`]; disable and remove end in
//! [`unwire_chat`]. A connector installed at runtime therefore receives
//! chat immediately, which the daemon's old per-connector `wire_*`
//! functions could never do.

use springtale_connector::chat::ChatMessage;
use springtale_core::rule::engine::TriggerEvent;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Start the chat receive loop for `name`, if the connector has one.
///
/// A no-op for connectors with no chat surface (`shell`, `github`, …)
/// and for a connector already wired — the loop is started once per
/// enable, never twice.
pub async fn wire_chat(state: &RuntimeState, name: &str) -> Result<(), OperationError> {
    let entry_host = {
        let registry = state.registry.read().await;
        let entry = registry
            .get(name)
            .ok_or_else(|| OperationError::Validation(format!("connector {name} not installed")))?;
        if !entry.enabled {
            return Ok(());
        }
        entry.host.clone()
    };
    let Some(source) = entry_host.chat_source() else {
        return Ok(());
    };
    if state.chat_tasks.contains_key(name) {
        return Ok(());
    }

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    state.chat_tasks.insert(name.to_owned(), stop_tx);

    // The source pushes into a private channel so this task can fan a
    // message out to BOTH consumers: the rule engine (ConnectorEvent
    // recipes, which the old daemon gateways emitted inline) and the
    // bot's chat path. The fan-out ends when the source drops its
    // sender, i.e. when `run` returns.
    let (raw_tx, mut raw_rx) = tokio::sync::mpsc::channel::<ChatMessage>(256);
    let bot_tx = state.chat_tx.clone();
    let trigger_registry = state.trigger_registry.clone();
    let fanout_name = name.to_owned();
    tokio::spawn(async move {
        while let Some(msg) = raw_rx.recv().await {
            if !msg.rule_events.is_empty()
                && let Some(registry) = trigger_registry.get()
            {
                let trigger_tx = registry.trigger_tx();
                for event in &msg.rule_events {
                    let evt = TriggerEvent {
                        trigger_type: "ConnectorEvent".to_owned(),
                        connector: Some(msg.connector.clone()),
                        event: Some(event.clone()),
                        payload: msg.raw.clone(),
                    };
                    if let Err(e) = trigger_tx.send(evt).await {
                        tracing::warn!(
                            connector = %fanout_name,
                            error = %e,
                            "failed to emit chat ConnectorEvent to rule engine"
                        );
                    }
                }
            }
            if msg.deliver_to_bot
                && let Err(e) = bot_tx.send(msg).await
            {
                tracing::warn!(
                    connector = %fanout_name,
                    error = %e,
                    "bot chat channel closed — dropping message"
                );
                break;
            }
        }
    });

    let run_name = name.to_owned();
    tokio::spawn(async move {
        match source.run(raw_tx, stop_rx).await {
            Ok(()) => tracing::info!(connector = %run_name, "chat source stopped"),
            Err(e) => {
                tracing::warn!(connector = %run_name, error = %e, "chat source stopped")
            }
        }
    });

    tracing::info!(connector = %name, "chat source wired");
    Ok(())
}

/// Stop the chat receive loop for `name`. A no-op when none is running.
pub fn unwire_chat(state: &RuntimeState, name: &str) {
    if let Some((_, stop)) = state.chat_tasks.remove(name) {
        let _ = stop.send(true);
        tracing::info!(connector = %name, "chat source unwired");
    }
}

/// Wire every installed, enabled connector that exposes a chat source.
/// Called once at init so a headless TOML deployment and a UI-configured
/// one take the same path.
pub async fn wire_all_chat(state: &RuntimeState) {
    let names: Vec<String> = {
        let registry = state.registry.read().await;
        registry
            .list()
            .into_iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(name, _)| name.to_owned())
            .collect()
    };
    for name in names {
        if let Err(e) = wire_chat(state, &name).await {
            tracing::warn!(connector = %name, error = %e, "failed to wire chat source");
        }
    }
}

/// Stop every running chat loop — used at shutdown so persistent
/// WebSocket / polling tasks drain instead of dying with the process.
pub fn unwire_all_chat(state: &RuntimeState) {
    let names: Vec<String> = state.chat_tasks.iter().map(|e| e.key().clone()).collect();
    for name in names {
        unwire_chat(state, &name);
    }
}

/// Send an outbound reply through the connector's own chat source.
///
/// Returns `false` when the connector has no chat source, so the caller
/// can fall back to the generic `send_message` action.
pub async fn send_chat(
    state: &RuntimeState,
    connector: &str,
    channel_id: &str,
    text: &str,
) -> Result<bool, OperationError> {
    let source = {
        let registry = state.registry.read().await;
        match registry.get(connector) {
            Some(entry) => entry.host.chat_source(),
            None => None,
        }
    };
    let Some(source) = source else {
        return Ok(false);
    };
    source
        .send(channel_id, text)
        .await
        .map_err(|e| OperationError::Connector(format!("failed to send via {connector}: {e}")))?;
    Ok(true)
}
