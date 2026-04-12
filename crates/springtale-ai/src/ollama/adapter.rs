use async_trait::async_trait;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ChatMessage, ConnectorInfo, TokenUsage,
    ToolCall, ToolDefinition,
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

impl OllamaAdapter {
    /// Sanitize a chat message list into an Ollama-shaped JSON array.
    /// Used by the tool-enabled path which needs `tool_calls` /
    /// `tool_call_id` fields the typed `OllamaChatMessage` struct
    /// doesn't carry.
    fn chat_to_json(&self, messages: Vec<ChatMessage>) -> Result<Vec<serde_json::Value>, AiError> {
        let mut out = Vec::with_capacity(messages.len());
        for m in messages {
            let sanitized = self.sanitize(&format!("chat.{}", m.role), &m.content)?;
            let mut obj = serde_json::Map::new();
            obj.insert("role".into(), serde_json::Value::String(m.role.clone()));

            if m.role == "tool" {
                // Ollama follows OpenAI's convention: role=tool +
                // tool_call_id + content.
                if let Some(id) = m.tool_call_id {
                    obj.insert("tool_call_id".into(), serde_json::Value::String(id));
                }
                obj.insert("content".into(), serde_json::Value::String(sanitized));
            } else if m.role == "assistant" && !m.tool_calls.is_empty() {
                obj.insert("content".into(), serde_json::Value::String(sanitized));
                let calls: Vec<serde_json::Value> = m
                    .tool_calls
                    .into_iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "function": {
                                "name": c.name,
                                "arguments": c.arguments,
                            }
                        })
                    })
                    .collect();
                obj.insert("tool_calls".into(), serde_json::Value::Array(calls));
            } else {
                obj.insert("content".into(), serde_json::Value::String(sanitized));
            }
            out.push(serde_json::Value::Object(obj));
        }
        Ok(out)
    }

    fn parse_ollama_tool_response(result: &serde_json::Value) -> AiResponse {
        let message = result.get("message");
        let content = message
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_owned();

        let mut tool_calls = Vec::new();
        if let Some(calls) = message.and_then(|m| m.get("tool_calls")).and_then(|v| v.as_array()) {
            for call in calls {
                let id = call
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_owned();
                let func = call.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_owned();
                // Ollama returns arguments as an object, not a string.
                let arguments = func
                    .and_then(|f| f.get("arguments"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                if !name.is_empty() {
                    // Synthesize an id when the model/tool-plugin omits it.
                    let id = if id.is_empty() {
                        format!("ollama_tool_{}", tool_calls.len())
                    } else {
                        id
                    };
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
            }
        }

        let done = result.get("done").and_then(|d| d.as_bool()).unwrap_or(true);
        let finish_reason = if done { Some("stop".to_owned()) } else { None };

        let usage = match (
            result.get("prompt_eval_count").and_then(|v| v.as_u64()),
            result.get("eval_count").and_then(|v| v.as_u64()),
        ) {
            (Some(prompt), Some(completion)) => Some(TokenUsage {
                prompt_tokens: prompt as u32,
                completion_tokens: completion as u32,
                total_tokens: (prompt + completion) as u32,
            }),
            _ => None,
        };

        AiResponse {
            content,
            finish_reason,
            usage,
            tool_calls,
        }
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
            tool_calls: Vec::new(),
        })
    }

    async fn complete_with_tools(
        &self,
        request: AiRequest,
        options: AiOptions,
        tools: &[ToolDefinition],
    ) -> Result<AiResponse, AiError> {
        if tools.is_empty() {
            return self.complete(request, options).await;
        }

        // Build a raw-JSON request so we can include `tools` —
        // OllamaChatRequest doesn't have that field.
        let messages_json = match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                vec![serde_json::json!({"role": "user", "content": sanitized})]
            }
            AiRequest::Chat { messages } => self.chat_to_json(messages)?,
        };

        let tool_json: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();

        let mut opts = serde_json::Map::new();
        if let Some(temp) = options.temperature {
            opts.insert("temperature".into(), serde_json::json!(temp));
        }
        opts.insert("num_predict".into(), serde_json::json!(options.max_tokens));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages_json,
            "tools": tool_json,
            "stream": false,
            "options": opts,
        });

        let result = tokio::time::timeout(options.timeout, self.client.chat_raw(&body))
            .await
            .map_err(|_| AiError::Timeout)??;

        Ok(Self::parse_ollama_tool_response(&result))
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
                        .filter(|c| !c.is_empty())
                    {
                        yield Ok(StreamChunk {
                            delta: content.to_owned(),
                            finish_reason: None,
                        });
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
