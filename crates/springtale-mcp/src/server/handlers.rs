//! `rmcp::ServerHandler` for [`SpringtaleMcp`].
//!
//! Thin: both methods forward to the inherent methods on the type so the
//! enforcement path stays testable without a `RequestContext`.

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as RmcpError, RoleServer, ServerHandler};

use super::registry::SpringtaleMcp;

impl ServerHandler for SpringtaleMcp {
    fn get_info(&self) -> ServerInfo {
        let name = match self.scope() {
            Some(connector) => format!("springtale/{connector}"),
            None => "springtale".to_owned(),
        };
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::new(name, env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "Springtale connector actions. Every tool is named \
             `{connector}.{action}` and every call crosses the sentinel, \
             the approval gate and the executions recorder — the same path \
             a rule action takes."
                .to_owned(),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, RmcpError> {
        Ok(ListToolsResult {
            tools: self.tools().await,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, RmcpError> {
        let arguments = request
            .arguments
            .map(serde_json::Value::Object)
            .unwrap_or(serde_json::Value::Null);
        self.dispatch_tool(&request.name, arguments).await
    }
}
