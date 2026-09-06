//! Shared operations — business logic called by both springtaled and desktop.
//!
//! Each module provides functions that take `&RuntimeState` and perform
//! complete operations (store + engine + validation + rollback).
//! Both apps are thin wrappers: springtaled wraps with HTTP,
//! desktop wraps with Tauri IPC.

pub mod agent;
pub mod approvals;
pub mod authors;
pub mod bot;
pub mod bot_settings;
pub mod canvas;
pub mod chat;
pub mod commands;
pub mod config;
pub mod connectors;
pub mod cross_channel;
pub mod data;
pub mod diagnostics;
pub mod error_fixes;
pub mod events;
pub mod executions;
pub mod formation_synthesis;
pub mod formations;
pub mod heartbeat;
pub mod memory;
pub mod migrate;
pub mod onboarding;
pub mod pairing;
pub mod preflight;
pub mod preview;
pub mod recipes;
pub mod rules;
pub mod safety;
pub mod sessions;
pub mod templates;
pub mod test_step;
pub mod travel;
pub mod vault;
pub mod workspaces;
