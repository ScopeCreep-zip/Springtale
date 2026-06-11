//! Shared ConnectorEvent emission for polling/streaming gateways.
//!
//! Polling gateways historically routed incoming provider events only to
//! the bot chat path (`bot_msg_tx`), so ConnectorEvent *recipes* never
//! fired from polling — they only worked in webhook mode. This helper
//! lets each gateway ALSO emit the event to the rule engine.
//!
//! The IRC/Nostr/Discord/Signal gateways already classify each event and
//! produce a payload carrying a `"trigger"` field plus the connector's
//! declared flat schema, so emission is uniform: forward
//! `ConnectorEvent{connector, event: payload.trigger, payload}` to the
//! engine. The payload is normalized centrally (identity for these
//! already-flat connectors) in the embedded trigger event loop before
//! rule matching.

use springtale_core::rule::engine::TriggerEvent;
use tokio::sync::mpsc;

/// Emit a ConnectorEvent to the rule engine for a gateway payload that
/// carries a `"trigger"` field (the gateway's event classification).
/// Spawns a detached send so the gateway dispatcher never blocks; a
/// no-op when the payload has no `trigger`.
pub fn emit_classified(
    trigger_tx: &mpsc::Sender<TriggerEvent>,
    connector: &'static str,
    payload: &serde_json::Value,
) {
    let Some(event) = payload.get("trigger").and_then(|t| t.as_str()) else {
        return;
    };
    let evt = TriggerEvent {
        trigger_type: "ConnectorEvent".to_owned(),
        connector: Some(connector.to_owned()),
        event: Some(event.to_owned()),
        payload: payload.clone(),
    };
    let tx = trigger_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = tx.send(evt).await {
            tracing::warn!(
                connector = connector,
                error = %e,
                "failed to emit gateway ConnectorEvent to rule engine"
            );
        }
    });
}
