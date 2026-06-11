//! Trigger lifecycle — `ConnectorEvent` subscription wiring shared by
//! every surface (daemon + desktop).

pub mod lifecycle;
pub mod registry;
pub mod wire;

pub use lifecycle::{activate_rule, deactivate_rule};
pub use registry::TriggerRegistry;
pub use wire::wire_connector_events;
