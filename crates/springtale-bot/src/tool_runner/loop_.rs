//! The actual AI ↔ tools loop. Kept in its own module so `mod.rs` stays
//! a table of contents and the builder functions are testable alone.

use std::sync::Arc;

use serde_json::json;
use springtale_ai::{
    AiAdapter, AiError, AiOptions, AiRequest, AiResponse, ChatMessage, ToolCall, ToolPolicy,
};
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::tier::WasmTier;
use springtale_runtime::CapabilityBridge;
use tokio::sync::RwLock;

use super::builder::{collect_tools, split_tool_name};

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
///
/// `formation_tier` scopes connector dispatch through the capability
/// checker (§16): `None` = permissive default (non-formation caller
/// — CLI, chat command); `Some(tier)` = bind every tool call to that
/// formation's momentum tier. The AI tool-use path only fires when a
/// formation is at Fever (`MomentumState::can_ai_orchestrate`), so this
/// is normally `Some(WasmTier::Fever)` in practice.
/// Bundled dependencies for the tool-use loop. Grouping the four
/// subsystems the runner touches (AI adapter, connector registry for
/// tool discovery, capability bridge for dispatch, sentinel for §6.10
/// gating) keeps `run_with_tools` at a signature the Rust project's
/// style actually accepts, and makes the daemon wiring one
/// `ToolRunnerDeps` clone instead of four coordinated parameters.
pub struct ToolRunnerDeps<'a> {
    pub adapter: &'a dyn AiAdapter,
    pub registry: &'a Arc<RwLock<ConnectorRegistry>>,
    pub bridge: &'a CapabilityBridge,
    pub sentinel: &'a Arc<springtale_sentinel::Sentinel>,
}

/// Per-invocation parameters — the AI request knobs plus the optional
/// formation tier binding. Kept separate from `ToolRunnerDeps` because
/// these change per call while the deps are stable for the lifetime of
/// the bot.
pub struct ToolRunnerCall<'a> {
    pub options: AiOptions,
    pub policy: &'a ToolPolicy,
    pub formation_tier: Option<WasmTier>,
}

pub async fn run_with_tools(
    deps: ToolRunnerDeps<'_>,
    mut messages: Vec<ChatMessage>,
    call: ToolRunnerCall<'_>,
) -> Result<AiResponse, ToolRunnerError> {
    // Tool list is still discovered via the registry (we need the
    // declared actions); execution goes through `dispatch_action*` so
    // sentinel evaluation (§6.10) runs before every network call.
    let tools = collect_tools(deps.registry, call.policy).await;
    let max_iterations = call.policy.effective_max_iterations();

    for iteration in 0..max_iterations {
        let request = AiRequest::Chat {
            messages: messages.clone(),
        };
        let response = deps
            .adapter
            .complete_with_tools(request, call.options.clone(), &tools)
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
            tool_name: None,
        });

        // Execute each call and push a `tool` result message.
        for tool_call in &response.tool_calls {
            let result = execute_tool_call(
                deps.bridge,
                deps.sentinel,
                tool_call,
                call.formation_tier,
            )
            .await;
            messages.push(result_message(tool_call, result));
        }
    }

    Err(ToolRunnerError::IterationLimit(max_iterations))
}

/// Result of a single tool execution as the model will see it.
struct ExecutedResult {
    body: String,
    is_error: bool,
}

async fn execute_tool_call(
    bridge: &CapabilityBridge,
    sentinel: &Arc<springtale_sentinel::Sentinel>,
    call: &ToolCall,
    formation_tier: Option<WasmTier>,
) -> ExecutedResult {
    let Some((connector, action)) = split_tool_name(&call.name) else {
        return ExecutedResult {
            body: format!("tool name '{}' is not in connector__action form", call.name),
            is_error: true,
        };
    };

    // Build a RunConnector action and dispatch through
    // `dispatch_action[_with_tier]` so sentinel evaluation runs before
    // the network call (§6.10 / Phase 17 / H1 fix).
    let params = match &call.arguments {
        serde_json::Value::Object(m) => m.clone(),
        _ => {
            let mut m = serde_json::Map::new();
            m.insert("arguments".into(), call.arguments.clone());
            m
        }
    };
    let action = springtale_core::rule::action::Action::RunConnector {
        connector: connector.to_owned(),
        action: action.to_owned(),
        params,
    };
    let outcome = match formation_tier {
        Some(tier) => {
            springtale_runtime::dispatch::dispatch_action_with_tier(
                &action, bridge, sentinel, tier,
            )
            .await
        }
        None => {
            springtale_runtime::dispatch::dispatch_action(&action, bridge, sentinel).await
        }
    };

    match outcome {
        Ok(msg) => {
            let payload = json!({ "message": msg });
            let mut body = payload.to_string();
            if body.len() > MAX_TOOL_OUTPUT_BYTES {
                body.truncate(MAX_TOOL_OUTPUT_BYTES);
                body.push_str("...[truncated]");
            }
            ExecutedResult {
                body,
                is_error: false,
            }
        }
        Err(e) => ExecutedResult {
            body: format!("{{\"error\": {}}}", serde_json::Value::String(e.to_string())),
            is_error: true,
        },
    }
}

fn result_message(call: &ToolCall, result: ExecutedResult) -> ChatMessage {
    let content = if result.is_error {
        format!("[ERROR] {}", result.body)
    } else {
        result.body
    };
    ChatMessage {
        role: "tool".into(),
        content,
        tool_calls: Vec::new(),
        tool_call_id: Some(call.id.clone()),   // OpenAI / Anthropic
        tool_name: Some(call.name.clone()),    // Ollama
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
