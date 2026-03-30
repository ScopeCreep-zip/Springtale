#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod actions;
pub mod config;
pub mod connector;
pub mod error;
pub mod triggers;
pub mod watcher;

pub use config::FilesystemConfig;
pub use connector::FilesystemConnector;
pub use error::FilesystemError;
