#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod actions;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod factory;

pub use client::HttpApi;
pub use config::HttpConfig;
pub use connector::HttpConnector;
pub use error::HttpError;
