use crate::error::AiError;

/// Maximum response body size: 10 MiB.
/// Prevents OOM from malicious or misconfigured AI endpoints.
pub const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

/// Validate that a base URL uses HTTPS (or is a localhost HTTP exception for dev).
///
/// Per spec (phase-2a.md line 42): "HTTPS validated (HTTP rejected unless
/// `--allow-insecure-ai-endpoint`)". Localhost is always allowed as HTTP
/// for Ollama and other local model runners.
pub fn validate_url_scheme(base_url: &str) -> Result<(), AiError> {
    if base_url.starts_with("https://") {
        return Ok(());
    }

    // Allow HTTP for localhost/127.0.0.1 (local model runners like Ollama)
    if base_url.starts_with("http://127.0.0.1")
        || base_url.starts_with("http://localhost")
        || base_url.starts_with("http://[::1]")
    {
        return Ok(());
    }

    Err(AiError::NotConfigured(format!(
        "AI endpoint must use HTTPS (got: {base_url}). \
         HTTP is only allowed for localhost (127.0.0.1, localhost, [::1])."
    )))
}

/// Read a response body with size validation.
///
/// Returns the body text if within limits, or `AiError::ResponseTooLarge`.
pub async fn read_response_body(response: reqwest::Response) -> Result<String, AiError> {
    // Check Content-Length header first (fast path)
    if let Some(content_length) = response.content_length()
        && content_length as usize > MAX_RESPONSE_SIZE
    {
        return Err(AiError::ResponseTooLarge {
            size: content_length as usize,
            limit: MAX_RESPONSE_SIZE,
        });
    }

    // Read body with size tracking
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AiError::InferenceFailed(format!("failed to read response body: {e}")))?;

    if bytes.len() > MAX_RESPONSE_SIZE {
        return Err(AiError::ResponseTooLarge {
            size: bytes.len(),
            limit: MAX_RESPONSE_SIZE,
        });
    }

    String::from_utf8(bytes.to_vec())
        .map_err(|e| AiError::Serialization(format!("response body is not valid UTF-8: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_https_allowed() {
        assert!(validate_url_scheme("https://api.openai.com").is_ok());
    }

    #[test]
    fn test_http_localhost_allowed() {
        assert!(validate_url_scheme("http://127.0.0.1:11434").is_ok());
        assert!(validate_url_scheme("http://localhost:11434").is_ok());
        assert!(validate_url_scheme("http://[::1]:11434").is_ok());
    }

    #[test]
    fn test_http_remote_rejected() {
        assert!(validate_url_scheme("http://api.example.com").is_err());
        assert!(validate_url_scheme("http://192.168.1.1:8080").is_err());
    }

    #[test]
    fn test_max_response_size() {
        assert_eq!(MAX_RESPONSE_SIZE, 10 * 1024 * 1024);
    }
}
