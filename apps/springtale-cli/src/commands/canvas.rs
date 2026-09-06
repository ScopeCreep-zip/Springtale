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
pub async fn run(stream: bool, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
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
    follow(response).await
}

/// Print each SSE `data:` payload as it arrives.
async fn follow(response: reqwest::Response) -> Result<()> {
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
            if !data.is_empty() {
                println!("{data}");
            }
        }
    }
    Ok(())
}
