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

    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream, AiError> {
        use crate::adapter::StreamChunk;
        use futures_util::StreamExt as _;

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

        let ollama_request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            options: Some(OllamaOptions {
                temperature: options.temperature,
                num_predict: Some(options.max_tokens),
            }),
            stream: Some(true),
        };

        // Send streaming request — returns raw response for NDJSON parsing
        let response = self.client.chat_stream(&ollama_request).await?;
        let mut byte_stream = response.bytes_stream();

        // Parse NDJSON: each line is {"message":{"content":"token"},"done":false}
        let stream = async_stream::stream! {
            let mut buffer = String::new();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Err(e) => {
                        yield Err(AiError::InferenceFailed(format!("stream read error: {e}")));
                        break;
                    }
                };
                buffer.push_str(&chunk);

                // Process complete lines (NDJSON — one JSON object per line)
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_owned();
                    buffer = buffer[newline_pos + 1..].to_owned();

                    if line.is_empty() { continue; }

                    let data: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let done = data.get("done").and_then(|d| d.as_bool()).unwrap_or(false);

                    if done {
                        yield Ok(StreamChunk {
                            delta: String::new(),
                            finish_reason: Some("stop".to_owned()),
                        });
                        return;
                    }

                    // Extract content from message.content
                    if let Some(content) = data
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !content.is_empty() {
                            yield Ok(StreamChunk {
                                delta: content.to_owned(),
                                finish_reason: None,
                            });
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream) as AiStream)
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
