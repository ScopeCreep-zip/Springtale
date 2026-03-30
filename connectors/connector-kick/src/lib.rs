#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod auth;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod triggers;
pub mod webhook;

pub use client::KickApi;
pub use config::KickConfig;
pub use connector::KickConnector;
pub use error::KickError;
