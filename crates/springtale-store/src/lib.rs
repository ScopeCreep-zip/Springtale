#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod backend;
pub mod error;
pub mod migrations;
pub mod paths;
pub mod queries;
pub mod schema;

pub use backend::SqliteBackend;
pub use backend::StorageBackend;
pub use error::StoreError;
pub use schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
pub use schema::connectors::ConnectorRow;
pub use schema::events::{EventEntry, EventFilter};
pub use schema::jobs::{JobId, JobRow};
