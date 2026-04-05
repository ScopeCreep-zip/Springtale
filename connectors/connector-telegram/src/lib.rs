#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod auth;
pub mod client;
pub mod config;
pub mod connector;
pub mod error;
pub mod polling;
pub mod triggers;
pub mod webhook;
pub mod factory;

pub use client::{TelegramApi, TelegramClient};
pub use config::TelegramConfig;
pub use connector::TelegramConnector;
pub use error::TelegramError;
