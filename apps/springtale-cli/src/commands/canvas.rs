//! `springtale canvas` — the colony as the UI sees it.
//!
//! A snapshot by default; `--stream` follows the daemon's multiplexed
//! SSE feed. The stream is ticket-authenticated (`POST /stream/ticket`)
//! so the bearer token never lands in a URL.

use anyhow::Result;
use serde_json::{Value, json};

use crate::client::Client;
use crate::output;

/// Print the canvas snapshot, or follow live updates.
pub async fn run(stream: bool, connections: bool, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    if connections {
        let body: Value = client.get("/canvas/connections").await?;
        return output::emit(json_out, &body, |v| {
            let rows = output::array(v, "connections")
                .iter()
                .map(|c| {
                    vec![
                        output::cell(c, "a"),
                        output::cell(c, "b"),
                        output::array(c, "pipes").len().to_string(),
                    ]
                })
                .collect();
            output::rows_table(&["FROM", "TO", "PIPES"], rows)
        });
    }
    if !stream {
        let body: Value = client.get("/canvas").await?;
        return output::emit(json_out, &body, |v| {
            serde_json::to_string_pretty(v).unwrap_or_default()
        });
    }

    let ticket: Value = client.post("/stream/ticket", &json!({})).await?;
    let ticket = ticket
        .get("ticket")
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow::anyhow!("daemon did not issue a stream ticket"))?;
    let response = client.stream(&format!("/stream?ticket={ticket}")).await?;
    follow(response, json_out).await
}

/// Print each SSE `data:` payload as it arrives. The payloads are already
/// JSON, so `--json` only decides pretty vs. one-line — but it still goes
/// through `output::emit`, so the flag has exactly one implementation.
async fn follow(response: reqwest::Response, json_out: bool) -> Result<()> {
    use anyhow::Context;
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("stream read error")?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));
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
            match serde_json::from_str::<Value>(&data) {
                Ok(event) => output::emit(json_out, &event, |v| v.to_string())?,
                // Unparseable frame: pass it through rather than drop it.
                Err(_) => output::emit(json_out, &data, |raw| raw.clone())?,
            }
        }
    }
    Ok(())
}
