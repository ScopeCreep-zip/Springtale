#![deny(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod capability;
pub mod connector;
pub mod error;
pub mod manifest;
pub mod native;
pub mod registry;
pub mod wasm;

pub use connector::trait_::{ActionResult, Connector, EventHandler};
pub use error::ConnectorError;
pub use manifest::types::{ActionDecl, Capability, ConnectorManifest, TriggerDecl};
pub use registry::store::ConnectorRegistry;
