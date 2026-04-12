#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod config;
pub mod connector;
pub mod error;
pub mod factory;
pub mod sandbox;

pub use config::ShellConfig;
pub use connector::ShellConnector;
pub use error::ShellError;
