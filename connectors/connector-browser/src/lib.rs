#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod auth;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod factory;
pub mod stealth;
pub mod triggers;

pub use config::{BrowserConfig, StealthProfile};
pub use connector::BrowserConnector;
pub use error::BrowserError;
