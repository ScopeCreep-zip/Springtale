#![deny(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod capability;
pub mod client;
pub mod config;
pub mod connector;
pub mod encoding;
pub mod error;
pub mod factory;
pub mod host;
pub mod manifest;
pub mod native;
pub mod registry;
#[cfg(feature = "wasm-sandbox")]
pub mod wasm;

pub use connector::subscription::{Subscription, SubscriptionCounter, SubscriptionId};
pub use connector::trait_::{ActionResult, Connector, EventHandler};
pub use error::ConnectorError;
pub use factory::{ConnectorFactory, FactoryEntry};
pub use host::ConnectorHost;
pub use manifest::types::{ActionDecl, Capability, ConnectorManifest, TriggerDecl};
pub use registry::store::ConnectorRegistry;
