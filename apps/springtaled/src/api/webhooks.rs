use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use springtale_core::rule::engine::TriggerEvent;
use springtale_store::schema::events::EventEntry;

use super::state::AppState;

/// Maximum JSON nesting depth for webhook payloads.
/// Prevents stack exhaustion from deeply nested structures.
const MAX_JSON_DEPTH: usize = 64;

/// POST /webhook/{connector}/{trigger} — receive an inbound webhook.
///
/// The management API receives webhook POSTs from external services (GitHub, Kick, etc.)
/// and routes them to the appropriate connector for signature verification and dispatch.
///
/// Flow:
/// 1. Look up connector in registry
/// 2. Connector-specific signature verification (GitHub: HMAC-SHA256, Kick: RSA)
/// 3. Dispatch trigger event to the rule engine via the trigger channel
pub async fn receive(
    State(state): State<AppState>,
    Path((connector_name, trigger_name)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    super::validate_path_param(&connector_name)?;
    super::validate_path_param(&trigger_name)?;

    let registry = state.runtime.registry.read().await;

    // Check connector exists and is enabled
    let entry = registry.get(&connector_name).ok_or(StatusCode::NOT_FOUND)?;

    if !entry.enabled {
        return Err(StatusCode::NOT_FOUND);
    }

    // Parse body as JSON and validate nesting depth
    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    if json_depth(&payload) > MAX_JSON_DEPTH {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Log the webhook receipt (without payload content per privacy model)
    tracing::info!(
        connector = %connector_name,
        trigger = %trigger_name,
        "webhook received"
    );

    // Verify webhook signature BEFORE dispatching.
    // Each connector implements verify_webhook() with its own scheme
    // (GitHub: HMAC-SHA256, Kick: RSA, Telegram: secret token).
    // Connectors that don't support webhooks reject with an error.
    let header_map: std::collections::HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_owned())))
        .collect();

    // Clone the host handle: the registry lock is dropped below, but the
    // connector still answers "what does this payload mean?" afterwards.
    let host = entry.host.clone();

    if let Err(e) = host.verify_webhook(&header_map, &body).await {
        tracing::warn!(
            connector = %connector_name,
            error = %e,
            "webhook signature verification failed"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Store event in log (metadata only, NOT payload content per privacy model)
    let event = EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: connector_name.clone(),
        trigger_type: trigger_name.clone(),
        action_taken: "webhook_received".to_owned(),
        timestamp: chrono::Utc::now(),
    };
    if let Err(e) = state.runtime.store.log_event(&event).await {
        tracing::warn!(error = %e, "failed to log webhook event");
    }

    // Broadcast to SSE subscribers (dashboard live event stream)
    let _ = state.event_tx.send(event);

    // Drop the registry lock before sending to the channel
    drop(registry);

    // Ask the connector what this verified payload means. The daemon
    // owns the transport (route, signature, event log); the connector
    // owns the protocol. This used to be a `match` on one connector
    // name here, so webhook chat worked for exactly that connector and
    // no other — Kick, whose chat only ever arrives by webhook, could
    // not reach the bot at all.
    //
    // Polling-mode gateways reach the same bot channel through their own
    // ChatSource loop (see runtime operations/connectors/chat.rs).
    let ingest = host
        .ingest_webhook(&trigger_name, &header_map, &payload)
        .await;

    for msg in ingest.messages {
        if !msg.deliver_to_bot {
            continue;
        }
        if let Err(e) = state.bot_msg_tx.try_send(msg) {
            tracing::warn!(
                connector = %connector_name,
                error = %e,
                "failed to forward webhook message to bot — may be dropped"
            );
        }
    }

    // Extra rule events the same request implies. The route's own
    // ConnectorEvent is dispatched below, so connectors return only
    // additional ones here.
    for extra in ingest.events {
        let evt = TriggerEvent {
            trigger_type: "ConnectorEvent".to_owned(),
            connector: Some(connector_name.clone()),
            event: Some(extra.event),
            payload: extra.payload,
        };
        if let Err(e) = state.trigger_tx.try_send(evt) {
            tracing::warn!(
                connector = %connector_name,
                error = %e,
                "failed to dispatch webhook-derived event"
            );
        }
    }

    // Acknowledge callback_query via answerCallbackQuery so the user's
    // inline-keyboard button stops spinning. Polling mode handles this
    // in runtime/connectors/telegram.rs; webhook mode needs it here.
    if trigger_name == "callback_query_received"
        && let Some(callback_id) = payload.get("id").and_then(|v| v.as_str())
    {
        let ack_input = serde_json::json!({
            "callback_query_id": callback_id,
        });
        let reg = state.runtime.registry.read().await;
        if let Err(e) = reg
            .execute(&connector_name, "answer_callback_query", ack_input)
            .await
        {
            tracing::warn!(
                error = %e,
                connector = %connector_name,
                "webhook: failed to answerCallbackQuery"
            );
        }
    }

    // Dispatch trigger event to the rule engine via the trigger channel.
    // This is the same path used by cron and file-watch triggers.
    // trigger_type must match what the rule engine expects for ConnectorEvent triggers.
    // See springtale_core::rule::engine::trigger_matches — it checks:
    //   event.trigger_type == "ConnectorEvent"
    //   event.connector == Some(connector_name)
    //   event.event == Some(event_name)
    // The raw payload is normalized to the connector's declared flat
    // trigger schema centrally, in the embedded trigger event loop (the
    // single chokepoint every ConnectorEvent passes through), so it's
    // forwarded raw here.
    let trigger_event = TriggerEvent {
        trigger_type: "ConnectorEvent".to_owned(),
        connector: Some(connector_name.clone()),
        event: Some(trigger_name.clone()),
        payload,
    };

    // Use try_send to avoid blocking if the trigger channel is full.
    // Returns 503 Service Unavailable instead of hanging indefinitely.
    if let Err(e) = state.trigger_tx.try_send(trigger_event) {
        match e {
            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                tracing::warn!("trigger channel full, dropping webhook event");
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                tracing::error!("trigger channel closed");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "received",
            "connector": connector_name,
            "trigger": trigger_name,
        })),
    ))
}

/// Calculate the maximum nesting depth of a JSON value.
/// Uses iterative stack to avoid stack overflow on deeply nested input.
fn json_depth(value: &serde_json::Value) -> usize {
    let mut max_depth = 0;
    let mut stack: Vec<(&serde_json::Value, usize)> = vec![(value, 1)];

    while let Some((val, depth)) = stack.pop() {
        if depth > max_depth {
            max_depth = depth;
        }
        match val {
            serde_json::Value::Object(map) => {
                for v in map.values() {
                    stack.push((v, depth + 1));
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    stack.push((v, depth + 1));
                }
            }
            _ => {}
        }
    }

    max_depth
}
