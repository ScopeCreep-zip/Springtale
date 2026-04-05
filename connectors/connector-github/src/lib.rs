#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod triggers;
pub mod webhook;
pub mod factory;

pub use client::GithubApi;
pub use config::GithubConfig;
pub use connector::GithubConnector;
pub use error::GithubError;
