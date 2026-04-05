#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod cache;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod factory;

pub use client::PresearchApi;
pub use config::PresearchConfig;
pub use connector::PresearchConnector;
pub use error::PresearchError;
