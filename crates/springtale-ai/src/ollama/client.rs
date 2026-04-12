use crate::error::AiError;

use super::types::{OllamaChatRequest, OllamaChatResponse, OllamaTagsResponse};

/// HTTP client for the Ollama API.
///
/// All calls go to `{base_url}/api/...`. No API key needed
/// (Ollama runs on localhost by default).
pub(crate) struct OllamaClient {
    http: reqwest::Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Result<Self, AiError> {
        crate::validate::validate_url_scheme(base_url)?;

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AiError::InferenceFailed(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    /// Non-streaming chat completion with response size validation.
    pub async fn chat(&self, request: &OllamaChatRequest) -> Result<OllamaChatResponse, AiError> {
        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("Ollama request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = crate::validate::read_response_body(response)
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(AiError::InferenceFailed(format!(
                "Ollama returned {status}: {body}"
            )));
        }

        // Read body with size validation (10 MiB limit)
        let body_text = crate::validate::read_response_body(response).await?;
        let parsed: OllamaChatResponse = serde_json::from_str(&body_text)
            .map_err(|e| AiError::Serialization(format!("failed to parse Ollama response: {e}")))?;

        Ok(parsed)
    }

    /// Non-streaming chat with a raw JSON body — used when tool-calling
    /// is enabled and the caller needs to set fields not on
    /// `OllamaChatRequest` (e.g. `tools`). Returns the parsed JSON value
    /// so the adapter can walk `message.tool_calls`.
    pub async fn chat_raw(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("Ollama request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = crate::validate::read_response_body(response)
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(AiError::InferenceFailed(format!(
                "Ollama returned {status}: {body}"
            )));
        }

        let body_text = crate::validate::read_response_body(response).await?;
        serde_json::from_str(&body_text)
            .map_err(|e| AiError::Serialization(format!("failed to parse Ollama response: {e}")))
    }

    /// Streaming chat request — returns the raw reqwest::Response for NDJSON parsing.
    pub async fn chat_stream(
        &self,
        request: &OllamaChatRequest,
    ) -> Result<reqwest::Response, AiError> {
        let url = format!("{}/api/chat", self.base_url);
        let mut body =
            serde_json::to_value(request).map_err(|e| AiError::Serialization(e.to_string()))?;
        body["stream"] = serde_json::json!(true);

        let response =
            self.http.post(&url).json(&body).send().await.map_err(|e| {
                AiError::InferenceFailed(format!("Ollama stream request failed: {e}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AiError::InferenceFailed(format!(
                "Ollama returned {status}: {body}"
            )));
        }

        Ok(response)
    }

    /// Check if Ollama is running by listing models.
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        match self.http.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    resp.json::<OllamaTagsResponse>().await.is_ok()
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = OllamaClient::new("http://127.0.0.1:11434");
        assert!(client.is_ok());
    }

    #[test]
    fn test_base_url_trimmed() {
        let client = OllamaClient::new("http://localhost:11434/").unwrap();
        assert_eq!(client.base_url, "http://localhost:11434");
    }

    #[test]
    fn test_rejects_remote_http() {
        let result = OllamaClient::new("http://remote-server.com:11434");
        assert!(result.is_err());
    }

    #[test]
    fn test_allows_https() {
        let result = OllamaClient::new("https://ollama.example.com");
        assert!(result.is_ok());
    }
}
