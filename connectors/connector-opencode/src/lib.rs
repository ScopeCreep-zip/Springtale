#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! OpenCode connector.
//!
//! Wraps a locally-running `opencode serve` daemon (default
//! `http://127.0.0.1:4096`) so a Springtale bot can hand off agentic
//! coding tasks — "fix this bug", "add tests", "refactor X" — and get the
//! agent's reply back. The daemon does the file edits and command runs on
//! the host; from Springtale's sandbox the connector only makes an HTTP
//! call to the loopback daemon. Coding actions are `read_only: false`, so
//! the W2 chat-approval gate fronts them.

pub mod actions;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod factory;

pub use client::OpenCodeApi;
pub use config::OpenCodeConfig;
pub use connector::OpenCodeConnector;
pub use error::OpenCodeError;
