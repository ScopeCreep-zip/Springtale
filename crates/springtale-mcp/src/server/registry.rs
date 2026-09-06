//! The daemon-wide MCP surface.
//!
//! The MCP transports spec says a Streamable HTTP server "operates as an
//! independent process that can handle multiple client connections". The
//! daemon is that process, so [`SpringtaleMcp`] exposes the *whole*
//! connector registry rather than one connector: every installed,
//! enabled connector's actions become tools named
//! `{connector}.{action}`.
//!
//! **There is no second execution path.** `call_tool` builds an
//! [`Action::RunConnector`] and hands it to
//! `springtale_runtime::dispatch::dispatch_action` — the same call a
//! rule action, a chat command and a formation tick make. The sentinel
//! (rate limit, circuit breaker, dead-man, toxic pairs), the approval
//! gate and the executions recorder therefore see an MCP call exactly as
//! they see a rule action. The capability check stays where it already
//! lives, inside `ConnectorHost::execute_checked`; this module adds none
//! of its own.

use rmcp::model::{CallToolResult, Content, Tool};

use springtale_cooperation::execution::{ExecutionContext, ExecutionMode};
use springtale_core::rule::RuleId;
use springtale_core::rule::action::Action;
use springtale_runtime::RuntimeState;

use crate::adapter::connector::action_to_tool;

/// Separator between the connector name and the action name in a tool
/// name (`github.create_issue`).
pub const TOOL_NAME_SEPARATOR: char = '.';

/// MCP server over the whole connector registry.
///
/// `scope` narrows the surface to a single connector — the thin
/// single-connector case that `ConnectorMcpServer` used to be a parallel
/// implementation of.
#[derive(Clone)]
pub struct SpringtaleMcp {
    pub(super) runtime: RuntimeState,
    pub(super) scope: Option<String>,
}

impl SpringtaleMcp {
    /// Serve every installed connector in the registry.
    pub fn new(runtime: RuntimeState) -> Self {
        Self {
            runtime,
            scope: None,
        }
    }

    /// Serve exactly one connector. Tools are still named
    /// `{connector}.{action}`, and calls still cross the sentinel: this
    /// is a filter over [`SpringtaleMcp::new`], not a second server.
    pub fn for_connector(runtime: RuntimeState, connector: impl Into<String>) -> Self {
        Self {
            runtime,
            scope: Some(connector.into()),
        }
    }

    /// The connector this server is scoped to, if any.
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }

    /// Whether `connector` is inside this server's scope.
    fn in_scope(&self, connector: &str) -> bool {
        self.scope.as_deref().is_none_or(|s| s == connector)
    }

    /// Every tool the registry currently offers.
    ///
    /// Disabled connectors are omitted — a disabled connector is not
    /// callable, so it must not be discoverable either. The list is built
    /// per request, not cached, so connectors installed or removed while
    /// the daemon runs show up on the next `tools/list`.
    pub async fn tools(&self) -> Vec<Tool> {
        let registry = self.runtime.registry.read().await;
        let mut tools = Vec::new();
        for (name, enabled) in registry.list() {
            if !enabled || !self.in_scope(name) {
                continue;
            }
            let Some(entry) = registry.get(name) else {
                continue;
            };
            for action in entry.host.actions() {
                tools.push(action_to_tool(name, action));
            }
        }
        tools
    }

    /// The manifest input schema for one tool, used for preflight
    /// validation. Not a security boundary — the sentinel and the
    /// capability checker are.
    async fn input_schema(&self, connector: &str, action: &str) -> Option<serde_json::Value> {
        let registry = self.runtime.registry.read().await;
        let entry = registry.get(connector)?;
        let decl = entry.host.actions().iter().find(|a| a.name == action)?;
        decl.input_schema.clone()
    }

    /// Execute one tool call through the shared dispatcher.
    ///
    /// Split out from the `ServerHandler` impl so the enforcement path is
    /// callable (and testable) without constructing a `RequestContext`.
    pub async fn dispatch_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let Some((connector, action_name)) = tool_name.split_once(TOOL_NAME_SEPARATOR) else {
            return Err(rmcp::ErrorData::invalid_params(
                format!(
                    "tool name must be `connector{TOOL_NAME_SEPARATOR}action`, got `{tool_name}`"
                ),
                None,
            ));
        };

        if !self.in_scope(connector) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("unknown tool `{tool_name}`"),
                None,
            ));
        }

        let params = match arguments {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            _ => {
                return Err(rmcp::ErrorData::invalid_params(
                    "tool arguments must be a JSON object".to_owned(),
                    None,
                ));
            }
        };

        // Preflight: the manifest's declared JSON Schema. Rejecting here
        // saves a dispatch round trip and gives the client a precise
        // error; it is not the authorization step.
        if let Some(schema) = self.input_schema(connector, action_name).await {
            let value = serde_json::Value::Object(params.clone());
            if let Err(e) = jsonschema::validate(&schema, &value) {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("input validation failed: {e}"),
                    None,
                ));
            }
        }

        let action = Action::RunConnector {
            connector: connector.to_owned(),
            action: action_name.to_owned(),
            params,
        };

        // One dispatch, the shared one. `ExecutionMode::Manual` because an
        // MCP call is a human-driven invocation, not a fired rule.
        let execution = ExecutionContext::for_global(RuleId::new(), ExecutionMode::Manual);

        tracing::debug!(
            tool = %tool_name,
            execution = %execution.execution_id,
            "MCP call_tool dispatching through the shared rule-action path"
        );

        let outcome = springtale_runtime::dispatch::dispatch_action(
            &action,
            &self.runtime.capability_bridge,
            &self.runtime.sentinel,
            execution,
            serde_json::Value::Null,
        )
        .await;

        match outcome {
            Ok(chain) => {
                let text = chain
                    .last_connector_output
                    .as_ref()
                    .and_then(|v| serde_json::to_string_pretty(v).ok())
                    .or_else(|| chain.last_connector_message.clone())
                    .unwrap_or_else(|| chain.brief());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            // Sentinel denial, approval refusal, capability denial and
            // connector failure all land here. MCP reports tool-level
            // failures in the result, not as a protocol error.
            Err(e) => {
                tracing::warn!(tool = %tool_name, error = %e, "MCP tool call refused or failed");
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }
}
