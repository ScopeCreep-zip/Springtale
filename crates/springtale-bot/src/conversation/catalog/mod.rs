//! Recipe catalogue projected for the conversational engine.

pub mod platform;
pub mod snapshot;

pub use platform::platform_docs;
pub use snapshot::{CatalogSnapshot, IntentDoc, SlotKindTag, SlotSpec};
