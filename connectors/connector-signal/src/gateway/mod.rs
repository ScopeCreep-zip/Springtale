use std::sync::Arc;

/// Listen for incoming Signal messages from signal-cli daemon.
///
/// signal-cli daemon exposes Server-Sent Events (SSE) at `/api/v1/events`.
/// Each event is a JSON-RPC notification containing a message envelope.
///
/// This gateway connects to the SSE stream and dispatches received
/// messages to the bot pipeline via the dispatcher callback.
pub async fn gateway_loop(
    daemon_url: String,
    dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    tracing::info!("Signal gateway loop started");
    let client = springtale_transport::safe_http::client().unwrap_or_default();

    loop {
        let events_url = format!("{daemon_url}/api/v1/events");
        tracing::info!(url = %events_url, "connecting to signal-cli SSE stream");

        // Connect to SSE endpoint
        let response = match client.get(&events_url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "failed to connect to signal-cli SSE");
                jittered_backoff().await;
                continue;
            }
        };

        if !response.status().is_success() {
            tracing::error!(
                status = %response.status(),
                "signal-cli SSE returned error status"
            );
            jittered_backoff().await;
            continue;
        }

        tracing::info!("signal-cli SSE connected");

        // Read SSE stream line by line
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        let mut buffer = String::new();

        loop {
            tokio::select! {
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            let text = String::from_utf8_lossy(&bytes);
                            buffer.push_str(&text);

                            // SSE events are separated by double newlines
                            while let Some(pos) = buffer.find("\n\n") {
                                let event_text = buffer[..pos].to_owned();
                                buffer = buffer[pos + 2..].to_owned();

                                if let Some(routed) = parse_sse_event(&event_text)
                                    .and_then(|payload| route_envelope(&payload))
                                {
                                    dispatcher(routed);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "SSE stream error");
                            break;
                        }
                        None => {
                            tracing::info!("SSE stream ended");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("Signal gateway shutting down");
                    return;
                }
            }
        }

        tracing::info!("reconnecting to signal-cli SSE...");
        jittered_backoff().await;
    }
}

/// Parse an SSE event into a JSON value.
/// SSE format: `data: {json}\n\n`
fn parse_sse_event(event_text: &str) -> Option<serde_json::Value> {
    for line in event_text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            return serde_json::from_str(data).ok();
        }
    }
    None
}

/// Route a signal-cli envelope to a trigger payload.
fn route_envelope(envelope: &serde_json::Value) -> Option<serde_json::Value> {
    // signal-cli wraps events in a JSON-RPC notification with method "receive"
    let result = envelope
        .get("params")
        .and_then(|p| p.get("result"))
        .or_else(|| envelope.get("result"))
        .unwrap_or(envelope);

    let env = result.get("envelope")?;
    let source = env
        .get("source")
        .or_else(|| env.get("sourceNumber"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    // Check for data message
    if let Some(data) = env.get("dataMessage") {
        let message = data.get("message").and_then(|m| m.as_str()).unwrap_or("");
        let timestamp = data.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0);
        let group_id = data
            .get("groupInfo")
            .and_then(|g| g.get("groupId"))
            .and_then(|g| g.as_str());
        let expires = data
            .get("expiresInSeconds")
            .and_then(|e| e.as_u64())
            .unwrap_or(0);

        let trigger = if group_id.is_some() {
            "group_message_received"
        } else {
            "message_received"
        };

        return Some(serde_json::json!({
            "trigger": trigger,
            "source": source,
            "user_id": source,
            "channel_id": group_id.unwrap_or(source),
            "message": message,
            "text": message,
            "timestamp": timestamp,
            "group_id": group_id,
            "expires_in_seconds": expires,
        }));
    }

    // Check for timer change
    if let Some(_timer) = env.get("typingMessage") {
        // Typing indicators — discard for privacy
        return None;
    }

    None
}

async fn jittered_backoff() {
    let base = std::time::Duration::from_secs(5);
    let jitter = std::time::Duration::from_millis(rand::random::<u64>() % 5000);
    tokio::time::sleep(base + jitter).await;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_event() {
        let event = "data: {\"jsonrpc\":\"2.0\",\"method\":\"receive\"}";
        let result = parse_sse_event(event);
        assert!(result.is_some());
        assert_eq!(result.unwrap()["method"], "receive");
    }

    #[test]
    fn test_parse_sse_event_no_data() {
        let event = "event: heartbeat";
        assert!(parse_sse_event(event).is_none());
    }

    #[test]
    fn test_route_1_to_1_message() {
        let envelope = serde_json::json!({
            "envelope": {
                "source": "+1234567890",
                "dataMessage": {
                    "message": "hello",
                    "timestamp": 1234567890,
                    "expiresInSeconds": 0
                }
            }
        });
        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "message_received");
        assert_eq!(result["source"], "+1234567890");
        assert_eq!(result["text"], "hello");
    }

    #[test]
    fn test_route_group_message() {
        let envelope = serde_json::json!({
            "envelope": {
                "source": "+1234567890",
                "dataMessage": {
                    "message": "group hello",
                    "timestamp": 1234567890,
                    "groupInfo": { "groupId": "abc123" }
                }
            }
        });
        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "group_message_received");
        assert_eq!(result["group_id"], "abc123");
    }
}
