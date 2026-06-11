use async_trait::async_trait;
use secrecy::SecretBox;

use crate::error::AiError;

/// Speech-to-text adapter trait.
///
/// Voice is a transport concern — the AI adapter always works with text
/// (SECURITY.md §1.3). STT bridges audio → text before the AI sees it.
///
/// WARNING: Cloud STT (OpenAI Whisper API) sends audio to a third party.
/// For privacy-critical use cases, deploy a local Whisper instance
/// (whisper.cpp, faster-whisper) and point the endpoint to localhost.
#[async_trait]
pub trait SttAdapter: Send + Sync {
    /// Transcribe audio data to text.
    ///
    /// `audio_data` is the raw audio bytes (wav, mp3, m4a, webm, etc.).
    /// `model` specifies which STT model to use (e.g., "whisper-1").
    async fn transcribe(&self, audio_data: &[u8], model: &str) -> Result<String, AiError>;
}

/// Whisper-compatible HTTP speech-to-text adapter.
///
/// Sends audio to a Whisper-compatible endpoint via multipart POST:
/// `POST {endpoint}/v1/audio/transcriptions`
///
/// Compatible with: OpenAI Whisper API, local whisper.cpp server,
/// faster-whisper server, any endpoint implementing the same interface.
pub struct WhisperHttpAdapter {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<SecretBox<String>>,
}

impl WhisperHttpAdapter {
    /// Create a new Whisper HTTP adapter.
    ///
    /// `endpoint` is the base URL (e.g., "https://api.openai.com" or
    /// "http://localhost:8080" for local).
    /// `api_key` is optional — required for OpenAI, not for local instances.
    /// Stays wrapped in `SecretBox` until the precise HTTP call site.
    pub fn new(endpoint: String, api_key: Option<SecretBox<String>>) -> Result<Self, AiError> {
        // Validate endpoint URL scheme
        crate::validate::validate_url_scheme(&endpoint)?;

        let client = springtale_transport::safe_http::builder()
            .timeout(std::time::Duration::from_secs(120)) // STT can be slow
            .build()
            .map_err(|e| AiError::NotConfigured(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            endpoint,
            api_key,
        })
    }
}

#[async_trait]
impl SttAdapter for WhisperHttpAdapter {
    async fn transcribe(&self, audio_data: &[u8], model: &str) -> Result<String, AiError> {
        let url = format!("{}/v1/audio/transcriptions", self.endpoint);

        let part = reqwest::multipart::Part::bytes(audio_data.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| AiError::InferenceFailed(format!("failed to create multipart: {e}")))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", model.to_owned());

        let mut request = self.client.post(&url).multipart(form);

        if let Some(ref key) = self.api_key {
            request = request.header(
                "Authorization",
                springtale_crypto::secret_use::bearer_header(key),
            );
        }

        let response = request
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("STT request failed: {e}")))?;

        // Enforce 10 MiB response limit
        let body = crate::validate::read_response_body(response).await?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| AiError::InferenceFailed(format!("STT response parse failed: {e}")))?;

        json.get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| AiError::InferenceFailed("STT response missing 'text' field".into()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_adapter_rejects_http() {
        let result = WhisperHttpAdapter::new("http://remote-server.com".into(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_whisper_adapter_allows_localhost_http() {
        let result = WhisperHttpAdapter::new("http://localhost:8080".into(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_whisper_adapter_allows_https() {
        let result = WhisperHttpAdapter::new("https://api.openai.com".into(), None);
        assert!(result.is_ok());
    }
}
