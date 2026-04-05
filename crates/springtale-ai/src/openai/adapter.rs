use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};
use serde::Deserialize;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ConnectorInfo, TokenUsage,
};
use crate::error::AiError;
use springtale_core::rule::types::Rule;

use super::client::OpenAiClient;

/// Configuration for the OpenAI-compatible adapter.
#[derive(Deserialize)]
pub struct OpenAiConfig {
    /// Base URL (e.g., "https://api.openai.com", "http://localhost:8080").
    pub base_url: String,
    /// API key wrapped in Secret<String>.
    #[serde(deserialize_with = "crate::config::deserialize_secret")]
    pub api_key: SecretBox<String>,
    /// Model name (e.g., "gpt-4o", "deepseek-chat").
    pub model: String,
}

impl std::fmt::Debug for OpenAiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .finish()
    }
}

/// AI adapter for OpenAI-compatible APIs.
///
/// Works with any endpoint implementing `/v1/chat/completions`:
/// OpenAI, Gemini, DeepSeek, OpenRouter, vLLM, llama.cpp server.
pub struct OpenAiCompatAdapter {
    client: OpenAiClient,
    model: String,
    sanitizer: crate::sanitize::Sanitizer,
}

impl OpenAiCompatAdapter {
    pub fn new(config: &OpenAiConfig) -> Result<Self, AiError> {
        // SECURITY: expose needed to clone API key into client
        let api_key = SecretBox::new(Box::new(config.api_key.expose_secret().clone()));
        let client = OpenAiClient::new(&config.base_url, api_key)?;
        Ok(Self {
            client,
            model: config.model.clone(),
            sanitizer: crate::sanitize::Sanitizer::default(),
        })
    }

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
impl AiAdapter for OpenAiCompatAdapter {
    async fn complete(
        &self,
        request: AiRequest,
        options: AiOptions,
    ) -> Result<AiResponse, AiError> {
        // Sanitize all message content before sending to AI (Layer 2 defense)
        let messages: Vec<serde_json::Value> = match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                vec![serde_json::json!({"role": "user", "content": sanitized})]
            }
            AiRequest::Chat { messages } => {
                let mut sanitized_msgs = Vec::with_capacity(messages.len());
                for m in messages {
                    let sanitized = self.sanitize(&format!("chat.{}", m.role), &m.content)?;
                    sanitized_msgs.push(serde_json::json!({"role": m.role, "content": sanitized}));
                }
                sanitized_msgs
            }
        };

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": options.max_tokens,
            "stream": false,
        });
        // Only include temperature when explicitly set (some providers reject null)
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let result = tokio::time::timeout(options.timeout, self.client.chat_completion(&body))
            .await
            .map_err(|_| AiError::Timeout)??;

        // Parse OpenAI response format
        let content = result
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_owned();

        let finish_reason = result
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("finish_reason"))
            .and_then(|r| r.as_str())
            .map(|s| s.to_owned());

        let usage = result.get("usage").and_then(|u| {
            Some(TokenUsage {
                prompt_tokens: u.get("prompt_tokens")?.as_u64()? as u32,
                completion_tokens: u.get("completion_tokens")?.as_u64()? as u32,
                total_tokens: u.get("total_tokens")?.as_u64()? as u32,
            })
        });

        Ok(AiResponse {
            content,
            finish_reason,
            usage,
        })
    }

    async fn stream(&self, _request: AiRequest, _options: AiOptions) -> Result<AiStream, AiError> {
        Err(AiError::InferenceFailed(
            "OpenAI streaming not yet implemented — use complete()".into(),
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
    fn test_config_debug_redacts_key() {
        let config = OpenAiConfig {
            base_url: "https://api.openai.com".into(),
            api_key: SecretBox::new(Box::new("sk-secret-key".into())),
            model: "gpt-4o".into(),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("sk-secret"));
    }
}
