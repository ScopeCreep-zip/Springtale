//! The Streamable HTTP MCP service, ready to mount on a router.
//!
//! `rmcp`'s `StreamableHttpService` is a Tower service, so the daemon
//! mounts it with `Router::nest_service`. Building it here keeps the
//! `rmcp` dependency inside this crate: `springtaled` wires transport and
//! auth, not MCP protocol types.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use springtale_runtime::RuntimeState;

use crate::server::SpringtaleMcp;

/// The concrete service type mounted at the daemon's MCP endpoint.
pub type SpringtaleHttpMcpService = StreamableHttpService<SpringtaleMcp, LocalSessionManager>;

/// Build the Streamable HTTP MCP service over the whole connector
/// registry.
///
/// Sessions are local to this process and are a transport correlator
/// only — the MCP security best practices are explicit that "MCP Servers
/// MUST NOT use sessions for authentication", so the daemon keeps its
/// bearer check on every request in front of this service.
pub fn streamable_http(runtime: RuntimeState) -> SpringtaleHttpMcpService {
    StreamableHttpService::new(
        move || Ok(SpringtaleMcp::new(runtime.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}
