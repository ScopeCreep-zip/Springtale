//! Shared operations — business logic called by both springtaled and desktop.
//!
//! Each module provides functions that take `&RuntimeState` and perform
//! complete operations (store + engine + validation + rollback).
//! Both apps are thin wrappers: springtaled wraps with HTTP,
//! desktop wraps with Tauri IPC.

pub mod agent;
pub mod canvas;
pub mod commands;
pub mod config;
pub mod connectors;
pub mod cross_channel;
pub mod data;
pub mod diagnostics;
pub mod error_fixes;
pub mod events;
pub mod formations;
pub mod memory;
pub mod migrate;
pub mod onboarding;
pub mod pairing;
pub mod rules;
pub mod safety;
pub mod templates;
pub mod travel;
pub mod vault;
