use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use springtale_core::rule::engine::TriggerEvent;
use springtale_store::backend::trait_::StorageBackend;
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

    let registry = state.registry.read().await;

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

    // Phase 1a: webhook signature verification will be connector-aware in Phase 2.
    // In production, this handler would:
    // 1. Read connector-specific signature headers
    // 2. Call the connector's verify function (GitHub HMAC, Kick RSA)
    // 3. Only dispatch after verification succeeds
    let _ = headers; // Used for signature verification in production

    // Store event in log (metadata only, NOT payload content per privacy model)
    let event = EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: connector_name.clone(),
        trigger_type: trigger_name.clone(),
        action_taken: "webhook_received".to_owned(),
        timestamp: chrono::Utc::now(),
    };
    if let Err(e) = state.store.log_event(&event).await {
        tracing::warn!(error = %e, "failed to log webhook event");
    }

    // Drop the registry lock before sending to the channel
    drop(registry);

    // Dispatch trigger event to the rule engine via the trigger channel.
    // This is the same path used by cron and file-watch triggers.
    // trigger_type must match what the rule engine expects for ConnectorEvent triggers.
    // See springtale_core::rule::engine::trigger_matches — it checks:
    //   event.trigger_type == "ConnectorEvent"
    //   event.connector == Some(connector_name)
    //   event.event == Some(event_name)
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
