use async_trait::async_trait;
use secrecy::SecretBox;

use crate::error::AiError;

/// Text-to-speech adapter trait.
///
/// Voice is a transport concern — the AI adapter always works with text
/// (SECURITY.md §1.3). TTS bridges text → audio after the AI produces it.
///
/// WARNING: Cloud TTS (ElevenLabs) sends text to a third party and may
/// log content for model improvement. For privacy-critical use cases,
/// deploy a local Piper TTS instance and point the endpoint to localhost.
#[async_trait]
pub trait TtsAdapter: Send + Sync {
    /// Synthesize text into audio data.
    ///
    /// Returns raw audio bytes (typically mp3 or wav format).
    /// `voice_id` selects the voice to use.
    async fn synthesize(&self, text: &str, voice_id: &str) -> Result<Vec<u8>, AiError>;
}

/// ElevenLabs-compatible HTTP text-to-speech adapter.
///
/// Sends text to an ElevenLabs-compatible endpoint:
/// `POST {endpoint}/v1/text-to-speech/{voice_id}`
///
/// Returns audio bytes (mp3 by default).
pub struct ElevenLabsAdapter {
    client: reqwest::Client,
    endpoint: String,
    api_key: SecretBox<String>,
}

impl ElevenLabsAdapter {
    /// Create a new ElevenLabs TTS adapter.
    ///
    /// `endpoint` is the base URL (e.g., "https://api.elevenlabs.io").
    /// `api_key` stays wrapped in `SecretBox` — only exposed at the
    /// precise HTTP call site (xi-api-key header).
    pub fn new(endpoint: String, api_key: SecretBox<String>) -> Result<Self, AiError> {
        crate::validate::validate_url_scheme(&endpoint)?;

        let client = springtale_transport::safe_http::builder()
            .timeout(std::time::Duration::from_secs(60))
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
impl TtsAdapter for ElevenLabsAdapter {
    async fn synthesize(&self, text: &str, voice_id: &str) -> Result<Vec<u8>, AiError> {
        let url = format!("{}/v1/text-to-speech/{voice_id}", self.endpoint);

        let body = serde_json::json!({
            "text": text,
            "model_id": "eleven_monolingual_v1",
        });

        let response = self
            .client
            .post(&url)
            .header(
                "xi-api-key",
                springtale_crypto::secret_use::header_value(&self.api_key),
            )
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("TTS request failed: {e}")))?;

        let status = response.status().as_u16();
        if status >= 400 {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".into());
            return Err(AiError::InferenceFailed(format!(
                "TTS API error ({status}): {error_body}"
            )));
        }

        // Read audio bytes with size limit
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("TTS response read failed: {e}")))?;

        // 10 MiB limit on audio response
        if bytes.len() > 10 * 1024 * 1024 {
            return Err(AiError::ResponseTooLarge {
                size: bytes.len(),
                limit: 10 * 1024 * 1024,
            });
        }

        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_elevenlabs_adapter_rejects_http() {
        let key = SecretBox::new(Box::new("key".to_owned()));
        let result = ElevenLabsAdapter::new("http://remote.com".into(), key);
        assert!(result.is_err());
    }

    #[test]
    fn test_elevenlabs_adapter_allows_https() {
        let key = SecretBox::new(Box::new("key".to_owned()));
        let result = ElevenLabsAdapter::new("https://api.elevenlabs.io".into(), key);
        assert!(result.is_ok());
    }
}
