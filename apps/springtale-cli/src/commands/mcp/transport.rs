//! HTTP framing for the daemon's Streamable HTTP MCP endpoint.
//!
//! The daemon answers a request with an SSE stream and a notification
//! with `202 Accepted`, and stamps `Mcp-Session-Id` on the response to
//! `initialize`. All three are transport concerns, so they are handled
//! here rather than leaking into the stdio loop. The session id is a
//! correlator the spec forbids treating as authentication — the bearer
//! token goes on every request regardless.

use anyhow::{Context, Result, anyhow};
use reqwest::Method;
use tokio::sync::Mutex;

use super::bridge::McpTransport;
use crate::client::Client;

/// The daemon's MCP endpoint path (mounted at the router root).
const MCP_PATH: &str = "/mcp";

/// Header the Streamable HTTP transport uses to correlate a session.
const SESSION_HEADER: &str = "mcp-session-id";

/// Both media types are mandatory on a POST — the server answers `406`
/// unless it can pick either.
const ACCEPT: &str = "application/json, text/event-stream";

/// Forwards stdio messages to springtaled over HTTP.
pub struct DaemonTransport {
    client: Client,
    session: Mutex<Option<String>>,
}

impl DaemonTransport {
    /// Wrap an authenticated management-API client. The base URL and API
    /// token are whatever every other subcommand resolved.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            session: Mutex::new(None),
        }
    }
}

impl McpTransport for DaemonTransport {
    async fn send(&self, message: String) -> Result<Option<String>> {
        let mut request = self
            .client
            .request(Method::POST, MCP_PATH)
            .header(reqwest::header::ACCEPT, ACCEPT)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(message);
        if let Some(session) = self.session.lock().await.as_deref() {
            request = request.header(SESSION_HEADER, session);
        }

        let response = request.send().await.context(crate::client::UNREACHABLE)?;

        // A session id only appears on the reply to `initialize`; keep
        // it for every later request.
        if let Some(session) = response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
        {
            *self.session.lock().await = Some(session.to_owned());
        }

        let status = response.status();
        if !status.is_success() {
            // The daemon's error bodies carry no credentials, but keep
            // the surface small anyway: status plus a short reason.
            return Err(anyhow!("daemon returned {status} for the MCP endpoint"));
        }
        if status == reqwest::StatusCode::ACCEPTED {
            // The answer to a notification: no body, no reply owed.
            return Ok(None);
        }

        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        let body = response.text().await.context("reading MCP response body")?;

        if is_sse {
            Ok(first_sse_data(&body))
        } else if body.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(body))
        }
    }
}

/// Pull the payload of the first SSE event out of a stream body.
///
/// An event ends at a blank line; its data is the `data:` field lines
/// joined with a newline. Keep-alive comments (`:` lines) and the
/// `event:`/`id:` fields are not payload.
fn first_sse_data(body: &str) -> Option<String> {
    let mut data: Vec<&str> = Vec::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if !data.is_empty() {
                break;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if data.is_empty() {
        None
    } else {
        Some(data.join("\n"))
    }
}
