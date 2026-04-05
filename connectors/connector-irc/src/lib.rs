#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod auth;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod gateway;
pub mod triggers;
pub mod factory;

pub use config::IrcConfig;
pub use connector::IrcConnector;
pub use error::IrcError;
