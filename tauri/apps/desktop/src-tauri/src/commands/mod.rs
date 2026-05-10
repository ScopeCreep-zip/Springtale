/// Tauri command handlers — thin validation layer.
///
/// Per ARCHITECTURE.md §9: "Thin layer. Validates inputs.
/// Delegates to crates. Never contains business logic directly."
///
/// Each module mirrors a springtale-runtime operations module.
/// No duplication — each command is defined in exactly one module.
pub mod agent;
pub mod authors;
pub mod bot;
pub mod canvas;
pub mod config;
pub mod connectors;
pub mod cooperation;
pub mod data;
pub mod diagnostics;
pub mod events;
pub mod fixes;
pub mod formations;
pub mod heartbeat;
pub mod memory;
pub mod onboarding;
pub mod panic;
pub mod rules;
pub mod safety;
pub mod send;
pub mod sessions;
pub mod templates;
pub mod travel;
pub mod vault;
