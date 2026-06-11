//! Typed client for the `opencode serve` HTTP API.
//!
//! Endpoints used (OpenCode OpenAPI, default port 4096):
//! - `POST /session` → create a session, returns a `Session` with `id`.
//! - `POST /session/:id/message` → send a prompt, returns
//!   `{ info: Message, parts: Part[] }`.
//!
//! All network calls live here (connector rule 7). Auth is HTTP Basic with
//! the fixed `opencode` username + the configured password, matching the
//! daemon's `OPENCODE_SERVER_PASSWORD`.

use async_trait::async_trait;
use secrecy::SecretBox;

use crate::config::{OPENCODE_USERNAME, OpenCodeConfig};
use crate::error::OpenCodeError;

/// The subset of the OpenCode API this connector drives.
#[async_trait]
pub trait OpenCodeApi: Send + Sync {
    /// Create a new session, returning its id.
    async fn create_session(&self, title: Option<&str>) -> Result<String, OpenCodeError>;

    /// Send a text prompt to a session and return the agent's combined
    /// text reply (concatenated text parts) plus the raw response.
    async fn send_prompt(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<serde_json::Value, OpenCodeError>;
}

/// HTTP client for a local `opencode serve` daemon.
pub struct OpenCodeClient {
    inner: reqwest::Client,
    base_url: String,
    password: Option<SecretBox<String>>,
    model: Option<String>,
    agent: Option<String>,
}

impl OpenCodeClient {
    pub fn new(config: OpenCodeConfig) -> Result<Self, OpenCodeError> {
        let inner = springtale_transport::safe_http::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| OpenCodeError::InvalidConfig(format!("failed to build client: {e}")))?;
        Ok(Self {
            inner,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            password: config.password,
            model: config.model,
            agent: config.agent,
        })
    }

    /// Apply the basic-auth header when a password is configured.
    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.password {
            Some(pw) => req.header(
                "Authorization",
                springtale_crypto::secret_use::basic_auth_header(OPENCODE_USERNAME, pw),
            ),
            None => req,
        }
    }
}

#[async_trait]
impl OpenCodeApi for OpenCodeClient {
    async fn create_session(&self, title: Option<&str>) -> Result<String, OpenCodeError> {
        let url = format!("{}/session", self.base_url);
        let mut body = serde_json::Map::new();
        if let Some(t) = title {
            body.insert("title".into(), serde_json::Value::String(t.to_owned()));
        }
        let response = self
            .with_auth(self.inner.post(&url))
            .json(&serde_json::Value::Object(body))
            .send()
            .await?;
        let json = handle_json(response).await?;
        json.get("id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| OpenCodeError::RequestFailed("session response missing id".into()))
    }

    async fn send_prompt(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<serde_json::Value, OpenCodeError> {
        let url = format!("{}/session/{session_id}/message", self.base_url);
        // `parts` is the required field — a single text part carries the prompt.
        let mut body = serde_json::json!({
            "parts": [ { "type": "text", "text": prompt } ],
        });
        if let Some(obj) = body.as_object_mut() {
            if let Some(model) = &self.model {
                obj.insert("model".into(), serde_json::Value::String(model.clone()));
            }
            if let Some(agent) = &self.agent {
                obj.insert("agent".into(), serde_json::Value::String(agent.clone()));
            }
        }
        let response = self
            .with_auth(self.inner.post(&url))
            .json(&body)
            .send()
            .await?;
        handle_json(response).await
    }
}

/// Concatenate the text parts of an opencode message response into the
/// agent's plain-text reply. Non-text parts (tool, file, reasoning) are
/// skipped — the action returns the raw response alongside this for callers
/// that need the structured parts.
pub fn extract_reply_text(response: &serde_json::Value) -> String {
    response
        .get("parts")
        .and_then(|p| p.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

async fn handle_json(response: reqwest::Response) -> Result<serde_json::Value, OpenCodeError> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_owned());
        return Err(OpenCodeError::RequestFailed(format!(
            "opencode daemon returned {}: {body}",
            status.as_u16()
        )));
    }
    response
        .json()
        .await
        .map_err(|e| OpenCodeError::RequestFailed(format!("failed to parse response: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn extract_reply_joins_text_parts_only() {
        let resp = serde_json::json!({
            "info": { "id": "m1" },
            "parts": [
                { "type": "text", "text": "Fixed the bug." },
                { "type": "tool", "name": "edit" },
                { "type": "text", "text": "Added a regression test." }
            ]
        });
        assert_eq!(
            extract_reply_text(&resp),
            "Fixed the bug.\nAdded a regression test."
        );
    }

    #[test]
    fn extract_reply_empty_when_no_text() {
        let resp = serde_json::json!({ "parts": [ { "type": "tool" } ] });
        assert_eq!(extract_reply_text(&resp), "");
    }
}
