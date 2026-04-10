use secrecy::{ExposeSecret, SecretBox};

use crate::error::AiError;

/// HTTP client for the Anthropic Messages API.
///
/// All calls go to `{base_url}/v1/messages`. Uses `x-api-key` header
/// (not Bearer token — Anthropic uses a different auth pattern).
pub(crate) struct AnthropicClient {
    http: reqwest::Client,
    base_url: String,
    api_key: SecretBox<String>,
}

impl AnthropicClient {
    pub fn new(base_url: &str, api_key: SecretBox<String>) -> Result<Self, AiError> {
        crate::validate::validate_url_scheme(base_url)?;

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AiError::InferenceFailed(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key,
        })
    }

    /// Non-streaming messages completion with response size validation.
    pub async fn messages(&self, body: &serde_json::Value) -> Result<serde_json::Value, AiError> {
        let url = format!("{}/v1/messages", self.base_url);

        // SECURITY: expose needed for Anthropic x-api-key header
        let response = self
            .http
            .post(&url)
            .header("x-api-key", self.api_key.expose_secret().as_str())
            .header("anthropic-version", "2023-06-01")
            .json(body)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("Anthropic request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = crate::validate::read_response_body(response)
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(AiError::InferenceFailed(format!(
                "Anthropic returned {status}: {body}"
            )));
        }

        // Read body with size validation (10 MiB limit)
        let body_text = crate::validate::read_response_body(response).await?;
        serde_json::from_str(&body_text)
            .map_err(|e| AiError::Serialization(format!("failed to parse Anthropic response: {e}")))
    }

    /// Streaming messages completion — returns a reqwest::RequestBuilder
    /// configured for SSE streaming. The caller wraps it with reqwest-eventsource.
    pub fn messages_stream_request(
        &self,
        body: &serde_json::Value,
    ) -> reqwest::RequestBuilder {
        let url = format!("{}/v1/messages", self.base_url);
        // SECURITY: expose needed for Anthropic x-api-key header
        self.http
            .post(&url)
            .header("x-api-key", self.api_key.expose_secret().as_str())
            .header("anthropic-version", "2023-06-01")
            .json(body)
    }

    /// Check if the API is reachable.
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/v1/messages", self.base_url);
        // SECURITY: expose needed for health check header
        match self
            .http
            .head(&url)
            .header("x-api-key", self.api_key.expose_secret().as_str())
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
        {
            // Anthropic returns 400 for HEAD (no body), but not 401/403 means auth works
            Ok(resp) => {
                let status = resp.status().as_u16();
                status != 401 && status != 403
            }
            Err(_) => false,
        }
    }
}
