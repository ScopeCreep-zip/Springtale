//! `rmcp::ServerHandler` for [`SpringtaleMcp`].
//!
//! Thin: both methods forward to the inherent methods on the type so the
//! enforcement path stays testable without a `RequestContext`.

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{NotificationContext, RequestContext};
use rmcp::{ErrorData as RmcpError, RoleServer, ServerHandler};

use super::notify::spawn_tool_list_forwarder;
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

    /// Start this client's `notifications/tools/list_changed` pump.
    ///
    /// `get_info` advertises `tools.listChanged`; this is where that
    /// promise is kept. One forwarder per initialized client, holding
    /// that client's peer and a fresh subscription to the runtime's
    /// tool-catalog fan-out. No replay: the client is about to call
    /// `tools/list` for the current state, so only changes *after*
    /// initialization matter. The task prunes itself when the client
    /// disconnects.
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        spawn_tool_list_forwarder(
            self.subscribe_tool_catalog(),
            context.peer,
            self.scope().map(str::to_owned),
        );
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
