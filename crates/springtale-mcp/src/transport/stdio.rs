use std::sync::Arc;

use rmcp::{ServerHandler, ServiceExt};

use crate::error::McpError;
use crate::server::ConnectorMcpServer;
use springtale_connector::Connector;
use springtale_connector::capability::grant::CapabilityChecker;

/// Start an MCP server on stdio (stdin/stdout).
///
/// This is the entry point for `springtale-cli mcp serve`. The server
/// reads JSON-RPC messages from stdin and writes responses to stdout,
/// following the MCP stdio transport specification.
///
/// `capability_checker` enforces the connector's declared capabilities
/// before every tool execution. The application layer (springtaled/CLI)
/// provides this from the `ConnectorRegistry`.
///
/// The server runs until the transport closes (stdin EOF) or an error occurs.
pub async fn start_stdio_server(
    connector: Arc<dyn Connector>,
    capability_checker: Arc<CapabilityChecker>,
) -> Result<(), McpError> {
    let server = ConnectorMcpServer::new(connector, capability_checker);
    let connector_name = server.get_info().server_info.name.clone();

    tracing::info!(
        connector = %connector_name,
        "starting MCP stdio server"
    );

    let service = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| McpError::Transport(format!("failed to start stdio server: {e}")))?;

    tracing::info!(
        connector = %connector_name,
        "MCP stdio server running"
    );

    service
        .waiting()
        .await
        .map_err(|e| McpError::Transport(format!("server error: {e}")))?;

    tracing::info!(
        connector = %connector_name,
        "MCP stdio server stopped"
    );

    Ok(())
}
