#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod adapter;
pub mod error;
pub mod server;
pub mod transport;

pub use error::McpError;
pub use server::ConnectorMcpServer;
pub use transport::start_stdio_server;
