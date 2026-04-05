//! Shared operations — business logic called by both springtaled and desktop.
//!
//! Each module provides functions that take `&RuntimeState` and perform
//! complete operations (store + engine + validation + rollback).
//! Both apps are thin wrappers: springtaled wraps with HTTP,
//! desktop wraps with Tauri IPC.

pub mod agent;
pub mod canvas;
pub mod connectors;
pub mod data;
pub mod events;
pub mod formations;
pub mod memory;
pub mod rules;
pub mod safety;
pub mod travel;
pub mod vault;
