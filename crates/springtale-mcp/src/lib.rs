#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod adapter;
pub mod error;
pub mod server;
pub mod service;

pub use error::McpError;
pub use server::{SpringtaleMcp, TOOL_NAME_SEPARATOR};
pub use service::{SpringtaleHttpMcpService, streamable_http};
