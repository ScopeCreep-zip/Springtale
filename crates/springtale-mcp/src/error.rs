use thiserror::Error;

/// Error type for the MCP protocol bridge.
#[derive(Debug, Error)]
pub enum McpError {
    /// MCP protocol-level error (malformed JSON-RPC, version mismatch).
    /// Used by Phase 2 SSE transport and protocol negotiation.
    #[error("MCP protocol error: {0}")]
    Protocol(String),

    /// A tool invocation failed at the connector layer.
    /// Used when wrapping ConnectorError for MCP error responses.
    #[error("tool invocation failed: {0}")]
    ToolInvocation(String),

    /// Transport-level error (stdio EOF, connection failure).
    #[error("transport error: {0}")]
    Transport(String),

    /// Input validation failed against the tool's JSON Schema.
    #[error("input validation failed: {0}")]
    ValidationFailed(String),
}
