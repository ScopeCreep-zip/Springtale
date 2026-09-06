//! `springtale trace` — thin SSE client for the daemon's `/stream`.
//!
//! Base URL, token derivation, and auth all live in `crate::client`.
//! This file only:
//!   1. Takes a stream ticket.
//!   2. Opens the SSE stream.
//!   3. Parses events and prints filtered lines.
//!
//! Per the "zero frontend logic" rule: if a new UI (Tauri, web) wants a
//! live event trace, it calls the same routes.

use anyhow::{Context, Result};
use serde_json::json;

use crate::client::Client;
use crate::output;

/// Real-time execution trace — connects to the daemon's SSE event stream
/// and prints rule triggers, action dispatches, and sentinel verdicts.
///
/// Usage:
///   springtale trace                        # all events
///   springtale trace --connector telegram   # filter by connector
///   springtale trace --rule my-rule         # filter by rule name
pub async fn run(
    connector_filter: Option<&str>,
    rule_filter: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let client = Client::from_config()?;

    // The SSE routes are ticket-authenticated, never bearer-in-URL:
    // `POST /stream/ticket` issues a single-use 30 s ticket.
    // security.md: "No secrets in URLs."
    let ticket: serde_json::Value = client.post("/stream/ticket", &json!({})).await?;
    let ticket = ticket
        .get("ticket")
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow::anyhow!("daemon did not issue a stream ticket"))?;

    // The banner is human chrome — under `--json` stdout carries only
    // events, so a consumer can parse the stream from the first byte.
    if !json_out {
        println!("Connecting to the event stream ...");
        println!("Press Ctrl+C to stop.\n");
    }

    let response = client.stream(&format!("/stream?ticket={ticket}")).await?;
    stream_events(response, connector_filter, rule_filter, json_out).await
}

async fn stream_events(
    response: reqwest::Response,
    connector_filter: Option<&str>,
    rule_filter: Option<&str>,
    json_out: bool,
) -> Result<()> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("stream read error")?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        // SSE events are separated by double newlines
        while let Some(pos) = buffer.find("\n\n") {
            let event_text: String = buffer.drain(..pos + 2).collect();

            let mut data = String::new();
            for line in event_text.lines() {
                if let Some(d) = line.strip_prefix("data: ") {
                    data.push_str(d);
                }
            }
            if data.is_empty() {
                continue;
            }

            let Ok(event) = serde_json::from_str::<serde_json::Value>(&data) else {
                continue;
            };

            if let Some(filter) = connector_filter
                && event.get("connector_name").and_then(|v| v.as_str()) != Some(filter)
            {
                continue;
            }
            if let Some(filter) = rule_filter
                && !event
                    .get("action_taken")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains(filter))
                    .unwrap_or(false)
            {
                continue;
            }

            output::emit(json_out, &event, format_event)?;
        }
    }

    Ok(())
}

fn format_event(event: &serde_json::Value) -> String {
    let timestamp = event
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "??:??:??".into());

    let connector = event
        .get("connector_name")
        .and_then(|v| v.as_str())
        .unwrap_or("system");
    let trigger_type = event
        .get("trigger_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let action = event
        .get("action_taken")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    format!("[{timestamp}] {trigger_type:15} {connector:25} {action}")
}
