use secrecy::{ExposeSecret, SecretBox};

use crate::error::AiError;

/// HTTP client for OpenAI-compatible /v1/chat/completions endpoints.
///
/// Works with: OpenAI, Gemini, DeepSeek, OpenRouter, vLLM, llama.cpp server.
pub(crate) struct OpenAiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: SecretBox<String>,
}

impl OpenAiClient {
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

    /// Non-streaming chat completion with response size validation.
    pub async fn chat_completion(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        // SECURITY: expose needed for Authorization Bearer header
        let response = self
            .http
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.expose_secret()),
            )
            .json(body)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("OpenAI request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = crate::validate::read_response_body(response)
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(AiError::InferenceFailed(format!(
                "OpenAI returned {status}: {body}"
            )));
        }

        // Read body with size validation (10 MiB limit)
        let body_text = crate::validate::read_response_body(response).await?;
        serde_json::from_str(&body_text)
            .map_err(|e| AiError::Serialization(format!("failed to parse OpenAI response: {e}")))
    }

    /// Streaming chat completion — returns the raw response for SSE parsing.
    pub async fn chat_completion_stream(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, AiError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        // SECURITY: expose needed for Authorization Bearer header
        let response = self
            .http
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.expose_secret()),
            )
            .json(body)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("OpenAI stream request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = crate::validate::read_response_body(response)
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(AiError::InferenceFailed(format!(
                "OpenAI returned {status}: {body}"
            )));
        }

        Ok(response)
    }

    /// Check if the endpoint is reachable by listing models.
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.base_url);
        // SECURITY: expose needed for Authorization header in health check
        match self
            .http
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.expose_secret()),
            )
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}
