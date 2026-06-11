use std::sync::Arc;

use rmcp::model::Tool;

use crate::adapter::connector::actions_to_tools;
use springtale_connector::Connector;
use springtale_connector::capability::grant::CapabilityChecker;

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
    pub(super) connector: Arc<dyn Connector>,
    pub(super) capability_checker: Arc<CapabilityChecker>,
    pub(super) tools: Vec<Tool>,
    pub(super) connector_name: String,
    pub(super) connector_version: String,
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
    pub(super) fn are_capabilities_approved(&self) -> bool {
        self.connector.manifest().capabilities.iter().all(|cap| {
            self.capability_checker
                .check(&self.connector_name, cap)
                .is_ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rmcp::ServerHandler;
    use springtale_connector::capability::grant::CapabilityPolicy;
    use springtale_connector::connector::trait_::{ActionResult, EventHandler};
    use springtale_connector::manifest::SignatureAlgorithm;
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
                            read_only: false,
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
                            read_only: false,
                            name: "ping".into(),
                            description: "Ping the service".into(),
                            input_schema: None,
                            output_schema: None,
                        },
                    ],
                    data_disclosure: vec![],
                    roles: vec![],
                    wasm_hash: None,
                    signature_alg: SignatureAlgorithm::default(),
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
        ) -> Result<springtale_connector::Subscription, springtale_connector::ConnectorError>
        {
            Ok(springtale_connector::Subscription {
                id: springtale_connector::SubscriptionId(0),
                trigger: String::new(),
            })
        }
        async fn remove_event(
            &self,
            _sub: &springtale_connector::Subscription,
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
