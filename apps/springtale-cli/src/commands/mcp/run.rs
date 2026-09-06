//! `springtale mcp serve` entry point.

use anyhow::Result;
use tokio::io::BufReader;

use super::bridge::bridge;
use super::transport::DaemonTransport;
use crate::client::Client;

/// Speak the MCP stdio transport on this process's stdin/stdout,
/// forwarding every message to the running daemon.
///
/// Runs until stdin reaches EOF. Nothing is written to stdout except
/// JSON-RPC messages — stdout *is* the transport — so diagnostics go to
/// stderr, which the MCP stdio spec reserves for logging.
pub async fn serve() -> Result<()> {
    let transport = DaemonTransport::new(Client::from_config()?);
    bridge(
        BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
        &transport,
    )
    .await
}
