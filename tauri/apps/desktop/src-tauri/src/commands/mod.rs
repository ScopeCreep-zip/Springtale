/// Tauri command handlers — thin validation layer.
///
/// Per ARCHITECTURE.md §9: "Thin layer. Validates inputs.
/// Delegates to crates. Never contains business logic directly."
///
/// Each module mirrors a springtale-runtime operations module.
/// No duplication — each command is defined in exactly one module.
pub mod agent;
pub mod approval;
pub mod authors;
pub mod bot;
pub mod canvas;
pub mod chat;
pub mod config;
pub mod connectors;
pub mod cooperation;
pub mod data;
pub mod diagnostics;
pub mod drift;
pub mod events;
pub mod executions;
pub mod fixes;
pub mod formations;
pub mod heartbeat;
pub mod memory;
pub mod onboarding;
pub mod panic;
pub mod quick_hide;
pub mod recipes;
pub mod rules;
pub mod bot_settings;
pub mod safety;
pub mod selector_picker;
pub mod send;
pub mod sessions;
pub mod templates;
pub mod test_step;
pub mod travel;
pub mod tray;
pub mod vault;
pub mod workspaces;
