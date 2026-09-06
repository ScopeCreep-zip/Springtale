//! `springtale mcp serve` — an MCP stdio bridge onto the daemon.
//!
//! The MCP server itself lives in `springtaled` behind the bearer check
//! (`apps/springtaled/src/api/mcp.rs`). Editors that can only launch a
//! subprocess speak the stdio transport, so this subcommand is the
//! adapter: newline-delimited JSON-RPC on its own stdin/stdout, HTTP to
//! the daemon in between.
//!
//! It holds no protocol logic — no tool list, no method dispatch, no
//! schema. Every message is forwarded verbatim; the only thing read out
//! of a message is whether it carries an `id`, which is framing (does a
//! reply get written?), not protocol.

pub mod bridge;
pub mod run;
pub mod transport;

pub use run::serve;
