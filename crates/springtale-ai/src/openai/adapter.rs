use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};
use serde::Deserialize;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ChatMessage, ConnectorInfo, TokenUsage,
    ToolCall, ToolDefinition,
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

impl OpenAiCompatAdapter {
    /// Translate cross-vendor `ChatMessage` list into the OpenAI
    /// `messages` shape, carrying through `tool_calls` on assistant
    /// turns and `tool_call_id` on `"tool"` role turns.
    fn build_messages(
        &self,
        request: AiRequest,
    ) -> Result<Vec<serde_json::Value>, AiError> {
        match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                Ok(vec![serde_json::json!({"role": "user", "content": sanitized})])
            }
            AiRequest::Chat { messages } => self.chat_to_openai(messages),
        }
    }

    fn chat_to_openai(&self, messages: Vec<ChatMessage>) -> Result<Vec<serde_json::Value>, AiError> {
        let mut out = Vec::with_capacity(messages.len());
        for m in messages {
            let sanitized = self.sanitize(&format!("chat.{}", m.role), &m.content)?;
            let mut obj = serde_json::Map::new();
            obj.insert("role".into(), serde_json::Value::String(m.role.clone()));

            if m.role == "tool" {
                let Some(tool_call_id) = m.tool_call_id else {
                    return Err(AiError::InferenceFailed(
                        "tool message missing tool_call_id".into(),
                    ));
                };
                obj.insert(
                    "tool_call_id".into(),
                    serde_json::Value::String(tool_call_id),
                );
                obj.insert("content".into(), serde_json::Value::String(sanitized));
            } else if m.role == "assistant" && !m.tool_calls.is_empty() {
                // Content may be empty when the model only emitted tool calls.
                if !sanitized.is_empty() {
                    obj.insert("content".into(), serde_json::Value::String(sanitized));
                } else {
                    obj.insert("content".into(), serde_json::Value::Null);
                }
                let calls: Vec<serde_json::Value> = m
                    .tool_calls
                    .into_iter()
                    .map(|c| {
                        let args_string = serde_json::to_string(&c.arguments)
                            .unwrap_or_else(|_| "{}".into());
                        serde_json::json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": args_string,
                            },
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

    fn build_request_body(
        &self,
        messages: Vec<serde_json::Value>,
        options: &AiOptions,
        tools: &[ToolDefinition],
        stream: bool,
    ) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": options.max_tokens,
            "stream": stream,
        });
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if !tools.is_empty() {
            let json_tools: Vec<serde_json::Value> = tools
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
            body["tools"] = serde_json::Value::Array(json_tools);
        }
        body
    }

    fn parse_openai_response(result: &serde_json::Value) -> AiResponse {
        let choice = result.get("choices").and_then(|c| c.get(0));
        let message = choice.and_then(|c| c.get("message"));

        let content = message
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_owned();

        let mut tool_calls = Vec::new();
        if let Some(calls) = message.and_then(|m| m.get("tool_calls")).and_then(|v| v.as_array()) {
            for call in calls {
                let id = call.get("id").and_then(|i| i.as_str()).unwrap_or("").to_owned();
                let func = call.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_owned();
                let arguments = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                if !id.is_empty() && !name.is_empty() {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
            }
        }

        let finish_reason = choice
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

        AiResponse {
            content,
            finish_reason,
            usage,
            tool_calls,
        }
    }
}

#[async_trait]
impl AiAdapter for OpenAiCompatAdapter {
    async fn complete(
        &self,
        request: AiRequest,
        options: AiOptions,
    ) -> Result<AiResponse, AiError> {
        self.complete_with_tools(request, options, &[]).await
    }

    async fn complete_with_tools(
        &self,
        request: AiRequest,
        options: AiOptions,
        tools: &[ToolDefinition],
    ) -> Result<AiResponse, AiError> {
        let messages = self.build_messages(request)?;
        let body = self.build_request_body(messages, &options, tools, false);

        let result = tokio::time::timeout(options.timeout, self.client.chat_completion(&body))
            .await
            .map_err(|_| AiError::Timeout)??;

        Ok(Self::parse_openai_response(&result))
    }

    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream, AiError> {
        use crate::adapter::StreamChunk;
        use futures_util::StreamExt as _;

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
            "stream": true,
        });
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let response = tokio::time::timeout(
            options.timeout,
            self.client.chat_completion_stream(&body),
        )
        .await
        .map_err(|_| AiError::Timeout)??;

        let mut byte_stream = response.bytes_stream();
        let stream = async_stream::stream! {
            let mut buffer = String::new();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Err(e) => {
                        yield Err(AiError::StreamError(format!("stream read error: {e}")));
                        break;
                    }
                };
                buffer.push_str(&chunk);

                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_owned();
                    buffer = buffer[newline_pos + 1..].to_owned();

                    if line.is_empty() { continue; }
                    let Some(data_str) = line.strip_prefix("data: ") else { continue };

                    if data_str == "[DONE]" {
                        yield Ok(StreamChunk {
                            delta: String::new(),
                            finish_reason: Some("stop".to_owned()),
                        });
                        return;
                    }

                    let data: serde_json::Value = match serde_json::from_str(data_str) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Only text content deltas are streamed. delta.tool_calls
                    // (argument JSON fragments indexed by tool_call.index) are
                    // intentionally not accumulated here. Tool-calling flows use
                    // complete_with_tools() (non-streaming) via tool_runner —
                    // streaming argument chars provides zero latency benefit since
                    // tool execution can't start until the arguments JSON is
                    // complete. If needed later: accumulate with
                    // HashMap<u32, (id, name, args_buffer)>, parse the final JSON
                    // when finish_reason == "tool_calls". See async-openai crate
                    // for the raw chunk types (ChatCompletionMessageToolCallChunk).
                    if let Some(content) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                        .filter(|c| !c.is_empty())
                    {
                        yield Ok(StreamChunk {
                            delta: content.to_owned(),
                            finish_reason: None,
                        });
                    }

                    if let Some(finish_reason) = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("finish_reason"))
                        .and_then(|f| f.as_str())
                    {
                        yield Ok(StreamChunk {
                            delta: String::new(),
                            finish_reason: Some(finish_reason.to_owned()),
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
