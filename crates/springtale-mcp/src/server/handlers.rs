use std::future::Future;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as RmcpError, RoleServer, ServerHandler};

use springtale_core::pipeline::context::PipelineContext;

use super::builder::ConnectorMcpServer;

impl ServerHandler for ConnectorMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                self.connector_name.clone(),
                self.connector_version.clone(),
            ))
            .with_instructions(format!(
                "Springtale connector: {}. {} tools available.",
                self.connector_name,
                self.tools.len()
            ))
    }

    /// List available tools, filtered by capability authorization.
    ///
    /// Discovery-time security layer: if the connector's capabilities are
    /// not all approved, return an empty tool list. The MCP client cannot
    /// discover tools for connectors with denied/pending capabilities.
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, RmcpError>> + Send + '_ {
        let authorized_tools = if self.are_capabilities_approved() {
            self.tools.clone()
        } else {
            tracing::info!(
                connector = %self.connector_name,
                "MCP list_tools: connector capabilities not fully approved, returning empty list"
            );
            Vec::new()
        };

        std::future::ready(Ok(ListToolsResult {
            tools: authorized_tools,
            next_cursor: None,
            meta: None,
        }))
    }

    /// Execute a tool call with three-layer enforcement.
    ///
    /// 1. JSON Schema validation against the tool's declared `input_schema`
    /// 2. Capability check via `check_action_capabilities()`
    /// 3. Connector execution via `connector.execute()`
    ///
    /// Defense-in-depth: capabilities are re-checked here even though
    /// `list_tools` filters by capability. Per research, discovery filtering
    /// alone is bypassable (direct HTTP calls skip tool discovery).
    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, RmcpError>> + Send + '_ {
        let connector = Arc::clone(&self.connector);
        let checker = Arc::clone(&self.capability_checker);
        let connector_name = self.connector_name.clone();
        let tool_name = request.name.clone();
        let arguments = request
            .arguments
            .map(serde_json::Value::Object)
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        // Capture the tool's input schema for validation
        let tool_schema = self
            .tools
            .iter()
            .find(|t| t.name.as_ref() == tool_name)
            .map(|t| serde_json::Value::Object((*t.input_schema).clone()));

        async move {
            // Create a PipelineContext for this invocation — provides trace ID
            // for audit trail correlation (ASI02: tool invocation sequence logging)
            // and ensures each MCP call has isolated context (cross-server shadowing)
            let ctx = PipelineContext::new(arguments.clone());

            tracing::debug!(
                trace_id = %ctx.trace_id,
                tool = %tool_name,
                connector = %connector_name,
                "MCP call_tool"
            );

            // Layer 1: JSON Schema validation
            if let Some(schema) = &tool_schema
                && let Err(e) = jsonschema::validate(schema, &arguments)
            {
                tracing::warn!(
                    trace_id = %ctx.trace_id,
                    tool = %tool_name,
                    error = %e,
                    "MCP tool input validation failed"
                );
                return Err(RmcpError::invalid_params(
                    format!("input validation failed: {e}"),
                    None,
                ));
            }

            // Layer 2: Capability enforcement (defense-in-depth)
            // Reuses springtale-connector's check_action_capabilities —
            // the same enforcement path as direct connector calls
            springtale_connector::native::capability::check_action_capabilities(
                &checker,
                connector.manifest(),
                &tool_name,
                &arguments,
            )
            .map_err(|e| {
                tracing::warn!(
                    trace_id = %ctx.trace_id,
                    tool = %tool_name,
                    connector = %connector_name,
                    error = %e,
                    "MCP capability check failed"
                );
                RmcpError::invalid_params(format!("capability denied: {e}"), None)
            })?;

            // Layer 3: Connector execution
            let result = connector
                .execute(&tool_name, arguments)
                .await
                .map_err(|e| {
                    RmcpError::internal_error(format!("connector execution failed: {e}"), None)
                })?;

            tracing::info!(
                trace_id = %ctx.trace_id,
                tool = %tool_name,
                connector = %connector_name,
                success = result.success,
                "MCP tool call completed"
            );

            if result.success {
                let output_text =
                    serde_json::to_string_pretty(&result.output).unwrap_or_else(|e| {
                        tracing::warn!(
                            trace_id = %ctx.trace_id,
                            tool = %tool_name,
                            error = %e,
                            "failed to serialize tool output"
                        );
                        result.message.clone()
                    });
                Ok(CallToolResult::success(vec![Content::text(output_text)]))
            } else {
                Ok(CallToolResult::error(vec![Content::text(result.message)]))
            }
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name.as_ref() == name).cloned()
    }
}
