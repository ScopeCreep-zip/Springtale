//! `springtale trace` — thin SSE client for the daemon's `/events/stream`.
//!
//! All config loading, token derivation, and auth formatting live in
//! `springtale_runtime::client_config`. This file only:
//!   1. Resolves a `ClientConfig` (base URL + token).
//!   2. Opens the SSE stream.
//!   3. Parses events and prints filtered lines.
//!
//! Per the "zero frontend logic" rule: if a new UI (Tauri, web) wants a
//! live event trace, it calls the same `client_config` helpers.

use anyhow::{Context, Result};
use secrecy::SecretString;

use springtale_runtime::client_config::{
    self, ClientConfigError, looks_like_hex_token, token_from_env, token_from_passphrase,
};

/// Real-time execution trace — connects to the daemon's SSE event stream
/// and prints rule triggers, action dispatches, and sentinel verdicts.
///
/// Usage:
///   springtale trace                        # all events
///   springtale trace --connector telegram   # filter by connector
///   springtale trace --rule my-rule         # filter by rule name
pub async fn run(connector_filter: Option<&str>, rule_filter: Option<&str>) -> Result<()> {
    let base_url = client_config::load_base_url(std::path::Path::new("springtale.toml"))
        .context("failed to load springtale.toml")?;
    let token = resolve_token()?;

    let url = format!("{base_url}/events/stream");
    println!("Connecting to {url} ...");
    println!("Press Ctrl+C to stop.\n");

    // Authorization header (never query param) so the token isn't logged
    // or exposed in process listings. security.md: "No secrets in URLs."
    let client =
        springtale_transport::safe_http::client().map_err(|e| anyhow::anyhow!("safe_http: {e}"))?;
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .context("failed to connect to event stream — is springtaled running?")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "event stream returned {}: check your API token",
            response.status()
        );
    }

    stream_events(response, connector_filter, rule_filter).await
}

/// Resolve the API token from env or interactive prompt.
///
/// Order: `SPRINGTALE_API_TOKEN` → `SPRINGTALE_PASSPHRASE` → TTY prompt.
/// A user-typed value is treated as a raw hex token if it looks like one,
/// otherwise it's passed through the same HMAC the daemon uses.
fn resolve_token() -> Result<String> {
    if let Some(token) = token_from_env() {
        return Ok(token);
    }
    let input = rpassword::read_password_from_tty(Some("API token (or vault passphrase): "))
        .map_err(|e| anyhow::anyhow!("failed to read token: {e}"))?;
    if input.is_empty() {
        return Err(anyhow::anyhow!(ClientConfigError::NoToken));
    }
    if looks_like_hex_token(&input) {
        Ok(input)
    } else {
        let secret = SecretString::new(input.into());
        Ok(token_from_passphrase(&secret))
    }
}

async fn stream_events(
    response: reqwest::Response,
    connector_filter: Option<&str>,
    rule_filter: Option<&str>,
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

            print_event(&event);
        }
    }

    Ok(())
}

fn print_event(event: &serde_json::Value) {
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

    println!("[{timestamp}] {trigger_type:15} {connector:25} {action}");
}
