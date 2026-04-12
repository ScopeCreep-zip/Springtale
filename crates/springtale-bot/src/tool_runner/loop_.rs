//! The actual AI ↔ tools loop. Kept in its own module so `mod.rs` stays
//! a table of contents and the builder functions are testable alone.

use std::sync::Arc;

use serde_json::json;
use springtale_ai::{
    AiAdapter, AiError, AiOptions, AiRequest, AiResponse, ChatMessage, ToolCall,
};
use springtale_connector::registry::store::ConnectorRegistry;
use tokio::sync::RwLock;

use super::builder::{collect_tools, split_tool_name};

/// Maximum conversation turns the runner will take before giving up.
/// Each iteration = one adapter round-trip + any tool executions it
/// asked for. Five is enough for "look up, then send" flows without
/// letting a bad loop exhaust the context window.
const MAX_ITERATIONS: usize = 5;

/// Truncate tool output fed back into the model. 8 KiB keeps the
/// conversation well under any vendor's context limit even after ~10
/// rounds of tool use.
const MAX_TOOL_OUTPUT_BYTES: usize = 8 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ToolRunnerError {
    #[error("ai error: {0}")]
    Ai(#[from] AiError),
    #[error("tool runner exceeded {0} iterations")]
    IterationLimit(usize),
}

/// Run an AI completion, executing any tool calls the model requests
/// against the connector registry and feeding the results back until
/// the model stops calling tools or the iteration cap is hit.
///
/// Returns the final `AiResponse` from the model. Tool-call turns are
/// invisible to the caller — they only see the final text. The
/// `messages` argument is consumed: it's appended-to during the loop.
pub async fn run_with_tools(
    adapter: &dyn AiAdapter,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    mut messages: Vec<ChatMessage>,
    options: AiOptions,
) -> Result<AiResponse, ToolRunnerError> {
    let tools = collect_tools(registry).await;

    for iteration in 0..MAX_ITERATIONS {
        let request = AiRequest::Chat {
            messages: messages.clone(),
        };
        let response = adapter
            .complete_with_tools(request, options.clone(), &tools)
            .await?;

        if response.tool_calls.is_empty() {
            return Ok(response);
        }

        tracing::info!(
            iteration = iteration,
            tool_calls = response.tool_calls.len(),
            "model requested tool execution"
        );

        // Append the assistant's tool-call turn so the next round has
        // the same transcript the model is operating against.
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: response.content.clone(),
            tool_calls: response.tool_calls.clone(),
            tool_call_id: None,
        });

        // Execute each call and push a `tool` result message.
        for call in &response.tool_calls {
            let result = execute_tool_call(registry, call).await;
            messages.push(result_message(call, result));
        }
    }

    Err(ToolRunnerError::IterationLimit(MAX_ITERATIONS))
}

/// Result of a single tool execution as the model will see it.
struct ExecutedResult {
    body: String,
    is_error: bool,
}

async fn execute_tool_call(
    registry: &Arc<RwLock<ConnectorRegistry>>,
    call: &ToolCall,
) -> ExecutedResult {
    let Some((connector, action)) = split_tool_name(&call.name) else {
        return ExecutedResult {
            body: format!("tool name '{}' is not in connector__action form", call.name),
            is_error: true,
        };
    };

    let reg = registry.read().await;
    match reg.execute(connector, action, call.arguments.clone()).await {
        Ok(result) => {
            let payload = json!({
                "message": result.message,
                "output": result.output,
            });
            let mut body = payload.to_string();
            if body.len() > MAX_TOOL_OUTPUT_BYTES {
                body.truncate(MAX_TOOL_OUTPUT_BYTES);
                body.push_str("...[truncated]");
            }
            ExecutedResult {
                body,
                is_error: !result.success,
            }
        }
        Err(e) => ExecutedResult {
            body: format!("{{\"error\": {}}}", serde_json::Value::String(e.to_string())),
            is_error: true,
        },
    }
}

fn result_message(call: &ToolCall, result: ExecutedResult) -> ChatMessage {
    // Vendor APIs signal failure differently (Anthropic has `is_error`
    // on tool_result blocks, OpenAI relies on the tool content). We use
    // a lowest-common-denominator `[ERROR]` prefix so every adapter
    // sees the same failure marker. When Anthropic's tool-result shape
    // is used, the adapter also sets `is_error: true` on the block.
    let content = if result.is_error {
        format!("[ERROR] {}", result.body)
    } else {
        result.body
    };
    ChatMessage {
        role: "tool".into(),
        content,
        tool_calls: Vec::new(),
        tool_call_id: Some(call.id.clone()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn result_message_preserves_id_and_role() {
        let call = ToolCall {
            id: "call-1".into(),
            name: "connector-x__ping".into(),
            arguments: json!({}),
        };
        let msg = result_message(
            &call,
            ExecutedResult {
                body: "pong".into(),
                is_error: false,
            },
        );
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(msg.content, "pong");
    }
}
