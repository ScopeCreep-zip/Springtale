use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Run the Slack Socket Mode event loop.
///
/// Socket Mode uses a WebSocket connection — no public HTTP endpoint needed.
/// This is local-first friendly: works behind firewalls, NAT, VPNs.
///
/// Protocol:
/// 1. POST apps.connections.open with App Token → get wss:// URL
/// 2. Connect WebSocket
/// 3. Receive JSON envelopes, acknowledge within 3 seconds
/// 4. Route events to triggers, dispatch to bot pipeline
/// 5. On disconnect: jittered backoff, reconnect
pub async fn gateway_loop(
    app_token: String,
    dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    tracing::info!("Slack Socket Mode gateway loop started");

    loop {
        // 1. Get WebSocket URL
        let ws_url = match get_socket_url(&app_token).await {
            Ok(url) => url,
            Err(e) => {
                tracing::error!(error = %e, "failed to get Socket Mode URL");
                jittered_backoff().await;
                continue;
            }
        };

        tracing::info!("connecting to Slack Socket Mode");

        // 2. Connect WebSocket
        let ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>> =
            match tokio_tungstenite::connect_async(&ws_url).await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    tracing::error!(error = %e, "WebSocket connection failed");
                    jittered_backoff().await;
                    continue;
                }
            };

        let (mut ws_tx, mut ws_rx) = ws_stream.split();
        tracing::info!("Slack Socket Mode connected");

        // 3. Event loop
        loop {
            tokio::select! {
                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(text.as_str()) {
                                // Acknowledge immediately (must be within 3 seconds)
                                if let Some(envelope_id) = envelope.get("envelope_id").and_then(|e| e.as_str()) {
                                    let ack = serde_json::json!({ "envelope_id": envelope_id }).to_string();
                                    if let Err(e) = ws_tx.send(Message::text(ack)).await {
                                        tracing::warn!(error = %e, "failed to send ack");
                                    }
                                }

                                // Route and dispatch
                                if let Some(payload) = route_envelope(&envelope) {
                                    dispatcher(payload);
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Slack Socket Mode connection closed by server");
                            break;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if let Err(e) = ws_tx.send(Message::Pong(data)).await {
                                tracing::warn!(error = %e, "failed to send pong");
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "WebSocket error");
                            break;
                        }
                        None => {
                            tracing::info!("WebSocket stream ended");
                            break;
                        }
                        _ => {} // Binary, Pong, Frame — ignore
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("Slack Socket Mode shutting down");
                    let _ = ws_tx.send(Message::Close(None)).await;
                    return;
                }
            }
        }

        // 4. Reconnect with jittered backoff
        tracing::info!("reconnecting to Slack Socket Mode...");
        jittered_backoff().await;
    }
}

/// Get a WebSocket URL from Slack's apps.connections.open API.
async fn get_socket_url(app_token: &str) -> Result<String, crate::error::SlackError> {
    let client = springtale_transport::safe_http::client()
        .map_err(|e| crate::error::SlackError::ConnectionFailed(format!("safe_http: {e}")))?;
    let response = client
        .post("https://slack.com/api/apps.connections.open")
        .header("Authorization", format!("Bearer {app_token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await
        .map_err(|e| {
            crate::error::SlackError::ConnectionFailed(format!(
                "apps.connections.open request failed: {e}"
            ))
        })?;

    let json: serde_json::Value = response.json().await.map_err(|e| {
        crate::error::SlackError::ConnectionFailed(format!(
            "apps.connections.open parse failed: {e}"
        ))
    })?;

    if json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let error = json
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown");
        return Err(crate::error::SlackError::AuthFailed(format!(
            "apps.connections.open: {error}"
        )));
    }

    json.get("url")
        .and_then(|u| u.as_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| {
            crate::error::SlackError::ConnectionFailed(
                "apps.connections.open response missing 'url' field".into(),
            )
        })
}

/// Route a Socket Mode envelope to a trigger payload.
fn route_envelope(envelope: &serde_json::Value) -> Option<serde_json::Value> {
    let envelope_type = envelope.get("type").and_then(|t| t.as_str())?;

    match envelope_type {
        "slash_commands" => {
            let payload = envelope.get("payload")?;
            Some(serde_json::json!({
                "trigger": "slash_command",
                "command": payload.get("command").and_then(|c| c.as_str()).unwrap_or(""),
                "text": payload.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                "user_id": payload.get("user_id").and_then(|u| u.as_str()).unwrap_or(""),
                "channel_id": payload.get("channel_id").and_then(|c| c.as_str()).unwrap_or(""),
                "response_url": payload.get("response_url").and_then(|r| r.as_str()),
            }))
        }
        "events_api" => {
            let payload = envelope.get("payload")?;
            let event = payload.get("event")?;
            let event_type = event.get("type").and_then(|t| t.as_str())?;

            match event_type {
                "message" => {
                    // Thread reply if thread_ts is present and different from ts
                    let ts = event.get("ts").and_then(|t| t.as_str()).unwrap_or("");
                    let thread_ts = event.get("thread_ts").and_then(|t| t.as_str());

                    let trigger = if thread_ts.is_some() && thread_ts != Some(ts) {
                        "thread_reply"
                    } else {
                        "message_received"
                    };

                    Some(serde_json::json!({
                        "trigger": trigger,
                        "user_id": event.get("user").and_then(|u| u.as_str()).unwrap_or(""),
                        "channel_id": event.get("channel").and_then(|c| c.as_str()).unwrap_or(""),
                        "text": event.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                        "ts": ts,
                        "thread_ts": thread_ts,
                    }))
                }
                "app_mention" => Some(serde_json::json!({
                    "trigger": "app_mentioned",
                    "user_id": event.get("user").and_then(|u| u.as_str()).unwrap_or(""),
                    "channel_id": event.get("channel").and_then(|c| c.as_str()).unwrap_or(""),
                    "text": event.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                    "ts": event.get("ts").and_then(|t| t.as_str()).unwrap_or(""),
                })),
                "reaction_added" => {
                    let item = event.get("item");
                    Some(serde_json::json!({
                        "trigger": "reaction_added",
                        "user_id": event.get("user").and_then(|u| u.as_str()).unwrap_or(""),
                        "reaction": event.get("reaction").and_then(|r| r.as_str()).unwrap_or(""),
                        "item_channel": item.and_then(|i| i.get("channel")).and_then(|c| c.as_str()).unwrap_or(""),
                        "item_ts": item.and_then(|i| i.get("ts")).and_then(|t| t.as_str()).unwrap_or(""),
                        "channel_id": item.and_then(|i| i.get("channel")).and_then(|c| c.as_str()).unwrap_or(""),
                    }))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Jittered backoff for reconnection.
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
    fn test_route_slash_command() {
        let envelope = serde_json::json!({
            "envelope_id": "abc123",
            "type": "slash_commands",
            "payload": {
                "command": "/remind",
                "text": "me in 5 minutes",
                "user_id": "U123",
                "channel_id": "C456"
            }
        });

        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "slash_command");
        assert_eq!(result["command"], "/remind");
        assert_eq!(result["text"], "me in 5 minutes");
        assert_eq!(result["user_id"], "U123");
    }

    #[test]
    fn test_route_message() {
        let envelope = serde_json::json!({
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "message",
                    "user": "U123",
                    "channel": "C456",
                    "text": "hello",
                    "ts": "1234567890.123456"
                }
            }
        });

        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "message_received");
        assert_eq!(result["user_id"], "U123");
    }

    #[test]
    fn test_route_thread_reply() {
        let envelope = serde_json::json!({
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "message",
                    "user": "U123",
                    "channel": "C456",
                    "text": "reply",
                    "ts": "1234567890.654321",
                    "thread_ts": "1234567890.123456"
                }
            }
        });

        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "thread_reply");
        assert_eq!(result["thread_ts"], "1234567890.123456");
    }

    #[test]
    fn test_route_app_mention() {
        let envelope = serde_json::json!({
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "app_mention",
                    "user": "U123",
                    "channel": "C456",
                    "text": "<@BOT> help",
                    "ts": "1234567890.123456"
                }
            }
        });

        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "app_mentioned");
    }

    #[test]
    fn test_route_reaction_added() {
        let envelope = serde_json::json!({
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "reaction_added",
                    "user": "U123",
                    "reaction": "thumbsup",
                    "item": {
                        "type": "message",
                        "channel": "C456",
                        "ts": "1234567890.123456"
                    }
                }
            }
        });

        let result = route_envelope(&envelope).unwrap();
        assert_eq!(result["trigger"], "reaction_added");
        assert_eq!(result["reaction"], "thumbsup");
    }

    #[test]
    fn test_route_unknown_type_returns_none() {
        let envelope = serde_json::json!({
            "type": "hello",
            "payload": {}
        });
        assert!(route_envelope(&envelope).is_none());
    }
}
