use async_trait::async_trait;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ConnectorInfo, TokenUsage,
};
use crate::error::AiError;
use springtale_core::rule::types::Rule;

use super::client::OllamaClient;
use super::types::{OllamaChatMessage, OllamaChatRequest, OllamaConfig, OllamaOptions};

/// AI adapter for Ollama (local model runner).
///
/// Connects to Ollama's HTTP API at localhost:11434 by default.
/// No API key required. Sanitizes all input before sending (Layer 2 defense).
pub struct OllamaAdapter {
    client: OllamaClient,
    model: String,
    sanitizer: crate::sanitize::Sanitizer,
}

impl OllamaAdapter {
    /// Create a new OllamaAdapter from config.
    pub fn new(config: OllamaConfig) -> Result<Self, AiError> {
        let client = OllamaClient::new(&config.base_url)?;
        Ok(Self {
            client,
            model: config.model,
            sanitizer: crate::sanitize::Sanitizer::default(),
        })
    }

    /// Sanitize message content. Returns sanitized text or blocks the request.
    fn sanitize(&self, field: &str, text: &str) -> Result<String, AiError> {
        let result = self.sanitizer.sanitize_text(field, text);
        if result.blocked {
            return Err(AiError::SanitizationBlocked {
                reason: result
                    .warnings
                    .first()
                    .map(|w| w.detail.clone())
                    .unwrap_or_else(|| "content blocked by sanitization policy".into()),
            });
        }
        Ok(result.text)
    }
}

#[async_trait]
impl AiAdapter for OllamaAdapter {
    async fn complete(
        &self,
        request: AiRequest,
        options: AiOptions,
    ) -> Result<AiResponse, AiError> {
        // Sanitize all message content before sending to AI (Layer 2 defense)
        let messages = match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                vec![OllamaChatMessage {
                    role: "user".into(),
                    content: sanitized,
                }]
            }
            AiRequest::Chat { messages } => {
                let mut sanitized_msgs = Vec::with_capacity(messages.len());
                for m in messages {
                    let sanitized_content =
                        self.sanitize(&format!("chat.{}", m.role), &m.content)?;
                    sanitized_msgs.push(OllamaChatMessage {
                        role: m.role,
                        content: sanitized_content,
                    });
                }
                sanitized_msgs
            }
        };

        let chat_request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: Some(false),
            options: Some(OllamaOptions {
                temperature: options.temperature,
                num_predict: Some(options.max_tokens),
            }),
        };

        let result = tokio::time::timeout(options.timeout, self.client.chat(&chat_request))
            .await
            .map_err(|_| AiError::Timeout)?
            .map_err(|e| AiError::InferenceFailed(e.to_string()))?;

        let content = result.message.map(|m| m.content).unwrap_or_default();

        let usage = match (result.prompt_eval_count, result.eval_count) {
            (Some(prompt), Some(completion)) => Some(TokenUsage {
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: prompt + completion,
            }),
            _ => None,
        };

        Ok(AiResponse {
            content,
            finish_reason: if result.done {
                Some("stop".into())
            } else {
                None
            },
            usage,
        })
    }

    async fn stream(&self, _request: AiRequest, _options: AiOptions) -> Result<AiStream, AiError> {
        // Phase 2a: streaming deferred — use complete() for now
        Err(AiError::InferenceFailed(
            "Ollama streaming not yet implemented — use complete()".into(),
        ))
    }

    async fn parse_rule(
        &self,
        intent: &str,
        available_connectors: &[ConnectorInfo],
    ) -> Result<Rule, AiError> {
        crate::parser::NlRuleParser::parse(self, intent, available_connectors, AiOptions::default())
            .await
    }

    async fn is_available(&self) -> bool {
        self.client.is_available().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation() {
        let adapter = OllamaAdapter::new(OllamaConfig::default());
        assert!(adapter.is_ok());
    }

    #[tokio::test]
    async fn test_is_available_when_not_running() {
        // Ollama not running on test machine — should return false, not panic
        let adapter = OllamaAdapter::new(OllamaConfig {
            base_url: "http://127.0.0.1:19999".into(), // unlikely port
            model: "test".into(),
        })
        .unwrap();
        assert!(!adapter.is_available().await);
    }
}
