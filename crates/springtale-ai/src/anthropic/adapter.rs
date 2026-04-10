use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};
use serde::Deserialize;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ConnectorInfo, TokenUsage,
};
use crate::error::AiError;
use springtale_core::rule::types::Rule;

use super::client::AnthropicClient;

/// Configuration for the Anthropic adapter.
#[derive(Deserialize)]
pub struct AnthropicConfig {
    /// API key wrapped in Secret<String>.
    #[serde(deserialize_with = "crate::config::deserialize_secret")]
    pub api_key: SecretBox<String>,
    /// Model name (e.g., "claude-sonnet-4-20250514").
    #[serde(default = "default_model")]
    pub model: String,
    /// Base URL. Default: "https://api.anthropic.com".
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_owned()
}

fn default_base_url() -> String {
    "https://api.anthropic.com".to_owned()
}

impl std::fmt::Debug for AnthropicConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicConfig")
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// AI adapter for the Anthropic Messages API.
///
/// Uses `/v1/messages` with `x-api-key` header. Supports Claude's
/// native `tool_use` content blocks for structured output.
pub struct AnthropicAdapter {
    client: AnthropicClient,
    model: String,
    sanitizer: crate::sanitize::Sanitizer,
}

impl AnthropicAdapter {
    pub fn new(config: &AnthropicConfig) -> Result<Self, AiError> {
        // SECURITY: expose needed to clone API key into client
        let api_key = SecretBox::new(Box::new(config.api_key.expose_secret().clone()));
        let client = AnthropicClient::new(&config.base_url, api_key)?;
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
impl AiAdapter for AnthropicAdapter {
    async fn complete(
        &self,
        request: AiRequest,
        options: AiOptions,
    ) -> Result<AiResponse, AiError> {
        // Sanitize all message content before sending to AI (Layer 2 defense)
        let (system, messages) = match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                (
                    None,
                    vec![serde_json::json!({"role": "user", "content": sanitized})],
                )
            }
            AiRequest::Chat { messages } => {
                let mut system_msg = None;
                let mut chat_msgs = Vec::new();
                for m in messages {
                    let sanitized = self.sanitize(&format!("chat.{}", m.role), &m.content)?;
                    if m.role == "system" {
                        system_msg = Some(sanitized);
                    } else {
                        chat_msgs.push(serde_json::json!({"role": m.role, "content": sanitized}));
                    }
                }
                (system_msg, chat_msgs)
            }
        };

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": options.max_tokens,
            "stream": false,
        });

        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys);
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let result = tokio::time::timeout(options.timeout, self.client.messages(&body))
            .await
            .map_err(|_| AiError::Timeout)??;

        // Parse Anthropic response — content is an array of blocks
        let content = result
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|blocks| {
                blocks.iter().find_map(|block| {
                    if block.get("type")?.as_str()? == "text" {
                        block.get("text")?.as_str().map(|s| s.to_owned())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default();

        let finish_reason = result
            .get("stop_reason")
            .and_then(|r| r.as_str())
            .map(|s| s.to_owned());

        let usage = result.get("usage").and_then(|u| {
            Some(TokenUsage {
                prompt_tokens: u.get("input_tokens")?.as_u64()? as u32,
                completion_tokens: u.get("output_tokens")?.as_u64()? as u32,
                total_tokens: (u.get("input_tokens")?.as_u64()?
                    + u.get("output_tokens")?.as_u64()?) as u32,
            })
        });

        Ok(AiResponse {
            content,
            finish_reason,
            usage,
        })
    }

    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream, AiError> {
        use crate::adapter::StreamChunk;
        use futures_util::StreamExt as _;

        // Build the request body (same as complete, but with stream: true)
        let (system, messages) = match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                (
                    None,
                    vec![serde_json::json!({"role": "user", "content": sanitized})],
                )
            }
            AiRequest::Chat { messages } => {
                let mut system_msg = None;
                let mut chat_msgs = Vec::new();
                for m in messages {
                    let sanitized = self.sanitize(&format!("chat.{}", m.role), &m.content)?;
                    if m.role == "system" {
                        system_msg = Some(sanitized);
                    } else {
                        chat_msgs.push(serde_json::json!({"role": m.role, "content": sanitized}));
                    }
                }
                (system_msg, chat_msgs)
            }
        };

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": options.max_tokens,
            "stream": true,
        });
        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys);
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        // Send streaming request to Anthropic
        let response = self
            .client
            .messages_stream_request(&body)
            .send()
            .await
            .map_err(|e| AiError::InferenceFailed(format!("Anthropic stream request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AiError::InferenceFailed(format!(
                "Anthropic returned {status}: {body}"
            )));
        }

        // Parse SSE from the response byte stream.
        // Anthropic sends text/event-stream with lines:
        //   event: content_block_delta
        //   data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}
        let mut byte_stream = response.bytes_stream();
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

                // Process complete SSE messages (separated by double newlines)
                while let Some(split_pos) = buffer.find("\n\n") {
                    let message = buffer[..split_pos].to_owned();
                    buffer = buffer[split_pos + 2..].to_owned();

                    // Extract the data line from the SSE message
                    let data_line = message
                        .lines()
                        .find(|l| l.starts_with("data: "))
                        .map(|l| &l[6..]);

                    let Some(data_str) = data_line else { continue };

                    let data: serde_json::Value = match serde_json::from_str(data_str) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let event_type = data.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match event_type {
                        "content_block_delta" => {
                            if let Some(text) = data
                                .get("delta")
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                yield Ok(StreamChunk {
                                    delta: text.to_owned(),
                                    finish_reason: None,
                                });
                            }
                        }
                        "message_delta" => {
                            let reason = data
                                .get("delta")
                                .and_then(|d| d.get("stop_reason"))
                                .and_then(|r| r.as_str())
                                .map(|s| s.to_owned());
                            if reason.is_some() {
                                yield Ok(StreamChunk {
                                    delta: String::new(),
                                    finish_reason: reason,
                                });
                            }
                        }
                        "message_stop" => {
                            yield Ok(StreamChunk {
                                delta: String::new(),
                                finish_reason: Some("stop".to_owned()),
                            });
                            return;
                        }
                        "error" => {
                            let msg = data
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                                .unwrap_or("unknown error");
                            yield Err(AiError::InferenceFailed(format!("Anthropic error: {msg}")));
                            return;
                        }
                        _ => {} // message_start, content_block_start, content_block_stop, ping
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
    fn test_config_debug_redacts_key() {
        let config = AnthropicConfig {
            api_key: SecretBox::new(Box::new("sk-ant-secret".into())),
            model: default_model(),
            base_url: default_base_url(),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("sk-ant"));
    }
}
