use std::sync::Arc;

use crate::adapter::AiAdapter;
use crate::anthropic::adapter::AnthropicConfig;
use crate::error::AiError;
use crate::noop::NoopAdapter;
use crate::ollama::types::OllamaConfig;
use crate::openai::adapter::OpenAiConfig;
use crate::{AnthropicAdapter, OllamaAdapter, OpenAiCompatAdapter};

/// Create an AI adapter from optional configs.
///
/// Priority: Anthropic > OpenAI > Ollama > NoopAdapter.
/// If all configs are `None`, returns NoopAdapter (AI disabled).
/// Only one adapter should be configured at a time.
pub fn create_adapter(
    ollama_config: Option<&OllamaConfig>,
    openai_config: Option<&OpenAiConfig>,
    anthropic_config: Option<&AnthropicConfig>,
) -> Result<Arc<dyn AiAdapter>, AiError> {
    if let Some(config) = anthropic_config {
        tracing::info!("initializing Anthropic AI adapter");
        return Ok(Arc::new(AnthropicAdapter::new(config)?));
    }

    if let Some(config) = openai_config {
        tracing::info!("initializing OpenAI-compatible AI adapter");
        return Ok(Arc::new(OpenAiCompatAdapter::new(config)?));
    }

    if let Some(config) = ollama_config {
        tracing::info!("initializing Ollama AI adapter");
        return Ok(Arc::new(OllamaAdapter::new(config.clone())?));
    }

    tracing::info!("no AI adapter configured — using NoopAdapter");
    Ok(Arc::new(NoopAdapter))
}

/// Verify the configured model's SHA-256 manifest digest matches the
/// user-pinned value. Idempotent: callers (the daemon boot path, the
/// CLI `ai swap` command) invoke this AFTER `create_adapter` returns,
/// before the adapter handles any user traffic. Maps to OWASP LLM03
/// (training-data / model-supply-chain poisoning).
///
/// Returns `Ok(())` when no digest is pinned (opt-in feature) OR when
/// the adapter has no concept of a local model store
/// (OpenAI/Anthropic — they own the manifest, the user can't audit
/// it). Errors only on a real Ollama mismatch.
pub async fn verify_model_pin(ollama_config: Option<&OllamaConfig>) -> Result<(), AiError> {
    let Some(config) = ollama_config else {
        return Ok(());
    };
    let Some(expected) = config.expected_digest.as_deref() else {
        return Ok(());
    };
    // The construction below is cheap (no network call); the digest
    // check itself is the one HTTP round-trip and the only place that
    // can fail closed for OWASP LLM03.
    let adapter = OllamaAdapter::new(config.clone())?;
    adapter.verify_digest(Some(expected)).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_when_all_none() {
        let adapter = create_adapter(None, None, None).unwrap();
        assert!(!adapter.is_available().await);
    }

    #[tokio::test]
    async fn test_ollama_created_when_configured() {
        let config = OllamaConfig {
            base_url: "http://127.0.0.1:19999".into(),
            model: "test".into(),
            expected_digest: None,
        };
        let adapter = create_adapter(Some(&config), None, None);
        assert!(adapter.is_ok());
    }

    #[tokio::test]
    async fn verify_model_pin_skips_when_unset() {
        // No expected_digest configured → skip the network call
        // entirely. Defends against the daemon startup hanging if
        // Ollama isn't running and the user hasn't opted into pinning.
        let config = OllamaConfig {
            base_url: "http://127.0.0.1:19999".into(),
            model: "test".into(),
            expected_digest: None,
        };
        assert!(verify_model_pin(Some(&config)).await.is_ok());
    }
}
