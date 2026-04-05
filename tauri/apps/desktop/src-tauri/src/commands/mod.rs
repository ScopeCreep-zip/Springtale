/// Tauri command handlers — thin validation layer.
///
/// Per ARCHITECTURE.md §9: "Thin layer. Validates inputs.
/// Delegates to crates. Never contains business logic directly."
///
/// Each command validates inputs, delegates to springtale-* crates,
/// and returns serializable results across the IPC boundary.
pub mod canvas;
pub mod connectors;
pub mod formations;
pub mod events;
pub mod panic;
pub mod rules;
pub mod safety;
pub mod travel;
pub mod vault;
