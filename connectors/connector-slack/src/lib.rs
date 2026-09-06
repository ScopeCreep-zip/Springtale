#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod actions;
pub mod auth;
pub mod chat;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod factory;
pub mod gateway;
pub mod mention;
pub mod triggers;

pub use chat::SlackChatSource;
pub use config::SlackConfig;
pub use connector::SlackConnector;
pub use error::SlackError;
