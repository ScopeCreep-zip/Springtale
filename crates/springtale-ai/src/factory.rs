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
        };
        let adapter = create_adapter(Some(&config), None, None);
        assert!(adapter.is_ok());
    }
}
