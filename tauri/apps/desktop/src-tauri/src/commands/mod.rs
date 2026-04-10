/// Tauri command handlers — thin validation layer.
///
/// Per ARCHITECTURE.md §9: "Thin layer. Validates inputs.
/// Delegates to crates. Never contains business logic directly."
///
/// Each module mirrors a springtale-runtime operations module.
/// No duplication — each command is defined in exactly one module.
pub mod agent;
pub mod authors;
pub mod canvas;
pub mod config;
pub mod connectors;
pub mod data;
pub mod events;
pub mod formations;
pub mod memory;
pub mod panic;
pub mod rules;
pub mod safety;
pub mod travel;
pub mod vault;
