#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Shared runtime for springtaled and the desktop app.
//!
//! Both apps are the same runtime — springtaled adds an HTTP API,
//! the desktop app adds a Tauri window. This crate provides:
//! - `init()` — shared boot sequence (store, engine, registry, AI, sentinel)
//! - `operations/` — shared business logic (rules, connectors, formations, etc.)
//! - `RuntimeState` — shared state both apps wrap
//!
//! Per ARCHITECTURE.md §9: "Core crates have zero Tauri dependency."

// Force linker to include connector crates so their inventory::submit!
// registrations are discovered by init_registry(). Without this, the
// linker dead-code-eliminates them because no symbol from these crates
// is directly referenced. See: dtolnay/inventory#7, rust-lang/rust#47384.
extern crate connector_bluesky;
extern crate connector_browser;
extern crate connector_discord;
extern crate connector_filesystem;
extern crate connector_github;
extern crate connector_http;
extern crate connector_irc;
extern crate connector_kick;
extern crate connector_nostr;
extern crate connector_presearch;
extern crate connector_shell;
extern crate connector_signal;
extern crate connector_slack;
extern crate connector_telegram;

pub mod client_config;
pub mod config;
pub mod cooperation;
pub mod dispatch;
pub mod error;
pub mod init;
pub mod operations;
pub mod state;

pub use client_config::{ClientConfig, ClientConfigError};
pub use config::{RuntimeConfig, StoreConfig};
pub use cooperation::{momentum_to_wasm_tier, BridgeError, CapabilityBridge};
pub use error::OperationError;
pub use init::init;
pub use state::{LiveFormationReader, RuntimeState};
