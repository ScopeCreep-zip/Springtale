use async_trait::async_trait;
use secrecy::SecretBox;
use serde::Deserialize;
use specta::Type;

use crate::adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ChatMessage, ConnectorInfo, TokenUsage,
    ToolCall, ToolDefinition,
};
use crate::error::AiError;
use springtale_core::rule::types::Rule;

use super::client::AnthropicClient;

/// Configuration for the Anthropic adapter.
#[derive(Deserialize, Type)]
pub struct AnthropicConfig {
    /// API key wrapped in Secret<String>. On the TS wire it appears as
    /// a plain `string` — `SecretBox` is only the in-process holder
    /// that prevents accidental logging.
    #[serde(deserialize_with = "crate::config::deserialize_secret")]
    #[specta(type = String)]
    pub api_key: SecretBox<String>,
    /// Model name (e.g., "claude-sonnet-4-6").
    #[serde(default = "default_model")]
    pub model: String,
    /// Base URL. Default: "https://api.anthropic.com".
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_model() -> String {
    // Keep this in sync with CLAUDE.md. The full family (as of
    // knowledge cutoff): claude-opus-4-6, claude-sonnet-4-6,
    // claude-haiku-4-5-20251001. Sonnet 4.6 is the default because
    // it's the best cost/quality mix for chat+tool-use workloads.
    "claude-sonnet-4-6".to_owned()
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
        let api_key = springtale_crypto::secret_use::clone_into_box(&config.api_key);
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

    /// Sibling-module accessor for the structured-extraction impl
    /// in `extractor.rs`. Routes source / hint text through the
    /// Layer-2 defense the rest of the adapter uses.
    pub(crate) fn sanitize_for_extractor(
        &self,
        field: &str,
        text: &str,
    ) -> Result<String, AiError> {
        self.sanitize(field, text)
    }

    pub(crate) fn anthropic_client(&self) -> &super::client::AnthropicClient {
        &self.client
    }

    pub(crate) fn anthropic_model(&self) -> &str {
        &self.model
    }
}

impl AnthropicAdapter {
    /// Translate our cross-vendor `ChatMessage` list into the Anthropic
    /// `messages` array shape, pulling out any `"system"` turn into the
    /// separate top-level `system` field. Handles text turns, assistant
    /// tool-call turns, and `"tool"` role tool-result turns.
    fn build_messages(
        &self,
        request: AiRequest,
    ) -> Result<(Option<String>, Vec<serde_json::Value>), AiError> {
        match request {
            AiRequest::Complete { prompt } => {
                let sanitized = self.sanitize("prompt", &prompt)?;
                Ok((
                    None,
                    vec![serde_json::json!({"role": "user", "content": sanitized})],
                ))
            }
            AiRequest::Chat { messages } => self.chat_to_anthropic(messages),
        }
    }

    fn chat_to_anthropic(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<(Option<String>, Vec<serde_json::Value>), AiError> {
        let mut system_msg: Option<String> = None;
        let mut out = Vec::with_capacity(messages.len());

        for m in messages {
            match m.role.as_str() {
                "system" => {
                    let sanitized = self.sanitize("chat.system", &m.content)?;
                    system_msg = match system_msg {
                        Some(existing) => Some(format!("{existing}\n\n{sanitized}")),
                        None => Some(sanitized),
                    };
                }
                "tool" => {
                    let Some(tool_call_id) = m.tool_call_id.clone() else {
                        return Err(AiError::InferenceFailed(
                            "tool message missing tool_call_id".into(),
                        ));
                    };
                    let sanitized = self.sanitize("chat.tool", &m.content)?;
                    // Anthropic expects tool results as user messages
                    // whose content is an array of tool_result blocks.
                    out.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": sanitized,
                        }],
                    }));
                }
                "assistant" if !m.tool_calls.is_empty() => {
                    let mut blocks = Vec::new();
                    if !m.content.is_empty() {
                        let sanitized = self.sanitize("chat.assistant", &m.content)?;
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": sanitized,
                        }));
                    }
                    for call in m.tool_calls {
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.name,
                            "input": call.arguments,
                        }));
                    }
                    out.push(serde_json::json!({
                        "role": "assistant",
                        "content": blocks,
                    }));
                }
                _ => {
                    let sanitized = self.sanitize(&format!("chat.{}", m.role), &m.content)?;
                    out.push(serde_json::json!({
                        "role": m.role,
                        "content": sanitized,
                    }));
                }
            }
        }

        Ok((system_msg, out))
    }

    fn build_request_body(
        &self,
        system: Option<String>,
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
        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys);
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if !tools.is_empty() {
            let json_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(json_tools);
        }
        body
    }

    fn parse_anthropic_response(result: &serde_json::Value) -> AiResponse {
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        if let Some(blocks) = result.get("content").and_then(|c| c.as_array()) {
            for block in blocks {
                let ty = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match ty {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if !content.is_empty() {
                                content.push('\n');
                            }
                            content.push_str(text);
                        }
                    }
                    "tool_use" => {
                        let id = block
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_owned();
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_owned();
                        let arguments = block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        if !id.is_empty() && !name.is_empty() {
                            tool_calls.push(ToolCall {
                                id,
                                name,
                                arguments,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

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

        AiResponse {
            content,
            finish_reason,
            usage,
            tool_calls,
        }
    }
}

#[async_trait]
impl AiAdapter for AnthropicAdapter {
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
        let (system, messages) = self.build_messages(request)?;
        let body = self.build_request_body(system, messages, &options, tools, false);

        let result = tokio::time::timeout(options.timeout, self.client.messages(&body))
            .await
            .map_err(|_| AiError::Timeout)??;

        Ok(Self::parse_anthropic_response(&result))
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
            .map_err(|e| {
                AiError::InferenceFailed(format!("Anthropic stream request failed: {e}"))
            })?;

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
                            // Only text_delta blocks are streamed. input_json_delta
                            // blocks (tool_use argument fragments) are intentionally
                            // not accumulated here. Tool-calling flows use the
                            // non-streaming complete_with_tools() via tool_runner,
                            // which needs complete tool calls before execution.
                            // Streaming individual argument chars has zero latency
                            // benefit since execution can't start until the JSON is
                            // complete. If needed later: accumulate with
                            // HashMap<index, (id, name, args_buffer)>, parse JSON
                            // at stream end when stop_reason == "tool_use".
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

    fn structured_extractor(&self) -> Option<&dyn crate::extractor::StructuredExtractor> {
        Some(self)
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
