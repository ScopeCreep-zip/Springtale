//! HTTP client for the springtaled management API.
//!
//! The CLI is a *client of the daemon*, not a second writer against the
//! same SQLite file. Every daemon-backed subcommand goes through this
//! type so an edit the CLI makes is immediately visible to the running
//! daemon (and to the desktop/web UIs reading the same runtime), and so
//! the CLI needs an API token rather than the vault passphrase.
//!
//! There is deliberately no fallback to opening the store directly: a
//! silent fallback is exactly how "the daemon never saw my edit" comes
//! back. When springtaled is not reachable, every call fails with
//! [`UNREACHABLE`], the way `docker`, `kubectl`, and `systemctl` do.
//!
//! The bearer comes from `springtale login` (plan 6.6) — a token the
//! daemon issued — never from the vault passphrase.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;

use springtale_runtime::client_config::{self, ClientConfigError, token_from_env};

/// The one message every daemon-backed subcommand fails with when
/// springtaled is not answering. Single string, single place.
pub const UNREACHABLE: &str =
    "cannot reach springtaled. Is it running? (`springtale server start`)";

/// Authenticated client for the management API.
pub struct Client {
    base: String,
    token: SecretString,
    http: reqwest::Client,
}

impl Client {
    /// Build a client from `springtale.toml` plus the resolved API token.
    pub fn from_config() -> Result<Self> {
        let base = client_config::load_base_url(Path::new("springtale.toml"))
            .context("springtale.toml")?;
        let token = resolve_token()?;
        let http =
            springtale_transport::safe_http::client().map_err(|e| anyhow!("safe_http: {e}"))?;
        Ok(Self { base, token, http })
    }

    /// Build a client against an explicit base URL and token — the seam
    /// the tests point at a stub server.
    #[cfg(test)]
    pub fn new(base: String, token: SecretString) -> Result<Self> {
        let http =
            springtale_transport::safe_http::client().map_err(|e| anyhow!("safe_http: {e}"))?;
        Ok(Self { base, token, http })
    }

    /// GET `path`, decoding the JSON body.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(self.http.get(self.url(path))).await
    }

    /// POST `path` with a JSON body, decoding the JSON response.
    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        self.send(self.http.post(self.url(path)).json(body)).await
    }

    /// PUT `path` with a JSON body, decoding the JSON response.
    pub async fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        self.send(self.http.put(self.url(path)).json(body)).await
    }

    /// DELETE `path`, decoding the JSON response.
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(self.http.delete(self.url(path))).await
    }

    /// DELETE `path` with a JSON body — the formation member-removal
    /// route takes the connector name in the body, not the path.
    pub async fn delete_with<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.send(self.http.delete(self.url(path)).json(body)).await
    }

    /// Open a streaming (SSE) response without decoding it. Used by
    /// `trace` and `canvas --stream`.
    pub async fn stream(&self, path: &str) -> Result<reqwest::Response> {
        // SECURITY: expose needed to set the bearer header.
        let resp = self
            .http
            .get(self.url(path))
            .bearer_auth(self.token.expose_secret())
            .send()
            .await
            .context(UNREACHABLE)?;
        if !resp.status().is_success() {
            bail!(
                "{}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        Ok(resp)
    }

    /// Start a request against `path` with the bearer header already
    /// applied, for callers that need the raw `reqwest` response rather
    /// than a decoded JSON body. The MCP stdio bridge uses it: it needs
    /// the response headers (`Mcp-Session-Id`) and an SSE body, and it
    /// must not hold a second copy of the API token.
    pub fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        // SECURITY: expose needed to set the bearer header.
        self.http
            .request(method, self.url(path))
            .bearer_auth(self.token.expose_secret())
    }

    fn url(&self, p: &str) -> String {
        format!("{}{p}", self.base)
    }

    async fn send<T: DeserializeOwned>(&self, rb: reqwest::RequestBuilder) -> Result<T> {
        // SECURITY: expose needed to set the bearer header.
        let resp = rb
            .bearer_auth(self.token.expose_secret())
            .send()
            .await
            .context(UNREACHABLE)?;
        if !resp.status().is_success() {
            bail!(
                "{}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        let body = resp.text().await.context("reading response body")?;
        if body.trim().is_empty() {
            // Some routes answer 200 with no body (`PUT /config/{key}`).
            return serde_json::from_str("null").context("empty response body");
        }
        serde_json::from_str(&body).with_context(|| format!("decoding response: {body}"))
    }
}

/// Resolve the API token the CLI authenticates with.
///
/// Order: `SPRINGTALE_API_TOKEN` → the token `springtale login` saved.
/// There is no passphrase path any more (plan 6.6): a passphrase is
/// exchanged for a token by `springtale login`, and only tokens the
/// daemon issued are accepted as bearers.
pub fn resolve_token() -> Result<SecretString> {
    if let Some(token) = token_from_env() {
        return Ok(SecretString::new(token.into()));
    }
    if let Some(saved) = client_config::read_token_file()? {
        return Ok(SecretString::new(saved.token.into()));
    }
    Err(anyhow!(ClientConfigError::NoToken))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Minimal HTTP/1.1 stub: answer one request with `body`, return the
    /// base URL it listens on.
    async fn stub_server(body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub server");
        let addr = listener.local_addr().expect("stub addr");
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        format!("http://{addr}")
    }

    fn client_for(base: String) -> Client {
        Client::new(base, SecretString::new("deadbeef".into())).expect("client")
    }

    #[tokio::test]
    async fn test_get_decodes_daemon_json_body() {
        let base = stub_server(r#"{"rules":[{"id":"r-1","name":"nightly"}]}"#).await;
        let client = client_for(base);
        let body: serde_json::Value = client.get("/rules").await.expect("get /rules");
        assert_eq!(body["rules"][0]["name"], "nightly");
    }

    #[tokio::test]
    async fn test_unreachable_daemon_reports_the_single_message() {
        // Bind then drop, so the port is (almost certainly) closed.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        drop(listener);

        let client = client_for(format!("http://{addr}"));
        let err = client
            .get::<serde_json::Value>("/rules")
            .await
            .expect_err("daemon is down");
        // No silent store fallback: one message, the top of the chain.
        assert_eq!(err.to_string(), UNREACHABLE);
    }
}
