use std::future::Future;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as RmcpError, RoleServer, ServerHandler};

use crate::adapter::connector::actions_to_tools;
use springtale_connector::Connector;
use springtale_connector::capability::grant::CapabilityChecker;
use springtale_core::pipeline::context::PipelineContext;

/// An MCP server that wraps a `Connector` with capability enforcement.
///
/// Any connector automatically becomes an MCP server without connector-side
/// changes. Tools are generated from the connector's `actions()` at
/// construction time and cached.
///
/// **Security model (two-layer authorization per codilime/OWASP pattern):**
///
/// 1. **Discovery time** (`list_tools`): tools are filtered by capability
///    status. Tools whose capabilities are denied or pending are hidden from
///    the MCP client entirely. The client cannot discover tools it cannot use.
///
/// 2. **Execution time** (`call_tool`): three enforcement layers run before
///    the connector is called:
///    - JSON Schema validation against the tool's declared `input_schema`
///    - Capability check via `check_action_capabilities()` from
///      `springtale-connector` (reuses the existing connector sandbox)
///    - Connector execution via `connector.execute()`
///
/// Per the architecture doc §6.8: "The connector sandbox IS the security
/// layer. MCP is a thin protocol bridge on top of it." We reuse the existing
/// capability enforcement, not invent a parallel framework.
///
/// Per pgEdge Zero Trust MCP: "Capability definitions must be enforced at
/// the MCP server boundary, not inside tool code."
pub struct ConnectorMcpServer {
    connector: Arc<dyn Connector>,
    capability_checker: Arc<CapabilityChecker>,
    tools: Vec<Tool>,
    connector_name: String,
    connector_version: String,
}

impl ConnectorMcpServer {
    /// Create a new MCP server wrapping the given connector with capability
    /// enforcement.
    ///
    /// `capability_checker` enforces the connector's declared capabilities.
    /// It is used at both discovery time (filtering tool list) and execution
    /// time (validating before each call).
    pub fn new(connector: Arc<dyn Connector>, capability_checker: Arc<CapabilityChecker>) -> Self {
        let manifest = connector.manifest();
        let tools = actions_to_tools(connector.actions());

        tracing::info!(
            connector = %manifest.name,
            tools = tools.len(),
            "MCP server created for connector"
        );

        Self {
            connector_name: manifest.name.clone(),
            connector_version: manifest.version.clone(),
            capability_checker,
            connector,
            tools,
        }
    }

    /// Get the number of tools this server exposes (before capability filtering).
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Check if the connector's capabilities are all approved.
    ///
    /// Used by `list_tools` to filter the tool list, and by `call_tool`
    /// for defense-in-depth. Returns true only if ALL declared capabilities
    /// pass the checker.
    fn are_capabilities_approved(&self) -> bool {
        self.connector.manifest().capabilities.iter().all(|cap| {
            self.capability_checker
                .check(&self.connector_name, cap)
                .is_ok()
        })
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use springtale_connector::capability::grant::CapabilityPolicy;
    use springtale_connector::connector::trait_::{ActionResult, EventHandler};
    use springtale_connector::manifest::types::{
        ActionDecl, Capability, ConnectorManifest, TriggerDecl,
    };

    struct TestConnector {
        manifest: ConnectorManifest,
    }

    impl TestConnector {
        fn new() -> Self {
            Self {
                manifest: ConnectorManifest {
                    name: "connector-test".into(),
                    version: "1.0.0".into(),
                    author: "test".into(),
                    description: "test connector".into(),
                    capabilities: vec![Capability::NetworkOutbound {
                        host: "api.example.com".into(),
                    }],
                    triggers: vec![],
                    actions: vec![
                        ActionDecl {
                            name: "search".into(),
                            description: "Search something".into(),
                            input_schema: Some(serde_json::json!({
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string" }
                                }
                            })),
                            output_schema: None,
                        },
                        ActionDecl {
                            name: "ping".into(),
                            description: "Ping the service".into(),
                            input_schema: None,
                            output_schema: None,
                        },
                    ],
                    data_disclosure: vec![],
                    wasm_hash: None,
                    signature: None,
                },
            }
        }
    }

    #[async_trait]
    impl Connector for TestConnector {
        fn triggers(&self) -> &[TriggerDecl] {
            &self.manifest.triggers
        }
        fn actions(&self) -> &[ActionDecl] {
            &self.manifest.actions
        }
        async fn execute(
            &self,
            action: &str,
            _input: serde_json::Value,
        ) -> Result<ActionResult, springtale_connector::ConnectorError> {
            Ok(ActionResult {
                success: true,
                output: serde_json::json!({"action": action, "result": "ok"}),
                message: "executed".into(),
            })
        }
        async fn on_event(
            &self,
            _trigger: &str,
            _handler: EventHandler,
        ) -> Result<(), springtale_connector::ConnectorError> {
            Ok(())
        }
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }
    }

    /// Create a server with AllowAll policy — all capabilities approved.
    fn setup_server_allowed() -> ConnectorMcpServer {
        let connector = Arc::new(TestConnector::new());
        let mut checker = CapabilityChecker::new();
        checker
            .register(
                "connector-test",
                &connector.manifest().capabilities,
                &CapabilityPolicy::AllowAll,
            )
            .ok();
        ConnectorMcpServer::new(connector, Arc::new(checker))
    }

    /// Create a server with DenyAll policy — all capabilities denied.
    fn setup_server_denied() -> ConnectorMcpServer {
        let connector = Arc::new(TestConnector::new());
        let mut checker = CapabilityChecker::new();
        checker
            .register(
                "connector-test",
                &connector.manifest().capabilities,
                &CapabilityPolicy::DenyAll,
            )
            .ok();
        ConnectorMcpServer::new(connector, Arc::new(checker))
    }

    #[test]
    fn test_server_creation() {
        let server = setup_server_allowed();
        assert_eq!(server.tool_count(), 2);
        assert_eq!(server.connector_name, "connector-test");
        assert_eq!(server.connector_version, "1.0.0");
    }

    #[test]
    fn test_get_info() {
        let server = setup_server_allowed();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "connector-test");
        assert_eq!(info.server_info.version, "1.0.0");
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }

    #[test]
    fn test_get_tool_found() {
        let server = setup_server_allowed();
        let tool = server.get_tool("search");
        assert!(tool.is_some());
        assert_eq!(tool.as_ref().map(|t| t.name.as_ref()), Some("search"));
    }

    #[test]
    fn test_get_tool_not_found() {
        let server = setup_server_allowed();
        assert!(server.get_tool("nonexistent").is_none());
    }

    #[test]
    fn test_cached_tools_match_actions() {
        let server = setup_server_allowed();
        assert_eq!(server.tools.len(), 2);
        assert_eq!(server.tools[0].name.as_ref(), "search");
        assert_eq!(server.tools[1].name.as_ref(), "ping");

        // search tool should have the declared schema
        assert!(server.tools[0].input_schema.contains_key("properties"));

        // ping tool should have the empty fallback schema
        assert_eq!(
            server.tools[1].input_schema.get("type"),
            Some(&serde_json::json!("object"))
        );
    }

    #[tokio::test]
    async fn test_execute_with_capabilities() {
        let server = setup_server_allowed();
        let result = server
            .connector
            .execute("search", serde_json::json!({"query": "test"}))
            .await;
        assert!(result.is_ok());
        assert!(result.as_ref().is_ok_and(|r| r.success));
    }

    #[test]
    fn test_input_validation_rejects_invalid() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        });
        let valid = serde_json::json!({"query": "hello"});
        let wrong_type = serde_json::json!({"query": 123});
        let missing_required = serde_json::json!({});

        assert!(jsonschema::validate(&schema, &valid).is_ok());
        assert!(jsonschema::validate(&schema, &wrong_type).is_err());
        assert!(jsonschema::validate(&schema, &missing_required).is_err());
    }

    #[test]
    fn test_capability_checker_integration() {
        let server = setup_server_allowed();
        assert!(server.are_capabilities_approved());

        let result = springtale_connector::native::capability::check_action_capabilities(
            &server.capability_checker,
            server.connector.manifest(),
            "search",
            &serde_json::json!({}),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_tools_filters_by_denied_capability() {
        let server = setup_server_denied();
        // With DenyAll, capabilities are not approved
        assert!(!server.are_capabilities_approved());
    }

    #[test]
    fn test_list_tools_shows_authorized() {
        let server = setup_server_allowed();
        assert!(server.are_capabilities_approved());
        assert_eq!(server.tools.len(), 2);
    }
}
