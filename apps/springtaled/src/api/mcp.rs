//! `/mcp` — the daemon's Streamable HTTP MCP endpoint.
//!
//! The MCP transports spec describes a Streamable HTTP server as one that
//! "operates as an independent process that can handle multiple client
//! connections" and that "MUST provide a single HTTP endpoint path
//! (hereafter referred to as the MCP endpoint) that supports both POST
//! and GET methods". `springtaled` is already that process, so the
//! endpoint lives here and covers the whole connector registry — not one
//! stdio subprocess per connector.
//!
//! Three spec security requirements are met by this module:
//!
//! - "Servers MUST validate the `Origin` header on all incoming
//!   connections to prevent DNS rebinding attacks" —
//!   [`auth::require_local_origin`], the outermost layer.
//! - "Servers SHOULD implement proper authentication for all
//!   connections" — [`auth::require_auth`], the daemon's existing bearer
//!   check, on *every* request rather than only on initialization. The
//!   `Mcp-Session-Id` header is a transport correlator and never
//!   authentication ("MCP Servers MUST NOT use sessions for
//!   authentication"); the token is the daemon's own API token, issued
//!   for this server, which satisfies "MCP servers MUST NOT accept any
//!   tokens that were not explicitly issued for the MCP server".
//! - "When running locally, servers SHOULD bind only to localhost" — the
//!   daemon binds `127.0.0.1` by default (see `main.rs`); this endpoint
//!   inherits that listener.

use axum::Router;
use axum::middleware;

use super::auth;
use super::state::AppState;

/// Build the `/mcp` router.
///
/// The handler is constructed per session and holds a clone of the shared
/// `RuntimeState`, so tool calls dispatch through the same sentinel,
/// approval gate and executions recorder as a rule action.
pub fn router(state: AppState) -> Router<AppState> {
    let service = springtale_mcp::streamable_http(state.runtime.clone());

    Router::new()
        .nest_service("/mcp", service)
        // Layers run outside-in in reverse declaration order: Origin is
        // checked first, then the bearer token, and only then does any
        // MCP framing get parsed.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .layer(middleware::from_fn(auth::require_local_origin))
}
