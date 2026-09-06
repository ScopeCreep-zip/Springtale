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
pub mod mention;
pub mod polling;
pub mod triggers;
pub mod webhook;

pub use chat::TelegramChatSource;
pub use client::{TelegramApi, TelegramClient};
pub use config::TelegramConfig;
pub use connector::TelegramConnector;
pub use error::TelegramError;
