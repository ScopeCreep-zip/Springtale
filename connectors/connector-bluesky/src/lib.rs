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
pub mod firehose;
pub mod gateway;
pub mod mention;
pub mod triggers;

pub use client::BlueskyApi;
pub use config::BlueskyConfig;
pub use connector::BlueskyConnector;
pub use error::BlueskyError;
