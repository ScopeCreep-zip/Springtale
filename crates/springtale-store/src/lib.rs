#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod backend;
pub mod error;
pub mod paths;
pub mod queries;
pub mod schema;

pub use backend::SqliteBackend;
pub use backend::StorageBackend;
pub use error::StoreError;
pub use schema::audit::{AuditEntry, AuditFilter};
pub use schema::bot::{MemoryRow, SessionRow, UserPrefsRow};
pub use schema::connectors::ConnectorRow;
pub use schema::cooperation::{CoopCasOutcome, CoopDepositRow, CoopWriteRow};
pub use schema::events::{EventEntry, EventFilter};
pub use schema::execution::ExecutionResultRow;
pub use schema::formations::{
    FormationMemberRow, FormationMomentumRow, FormationRallyRow, FormationRow,
};
pub use schema::jobs::{JobId, JobRow};
pub use schema::mental_model::{
    MentalModelBundle, MentalModelCapabilityRow, MentalModelConventionRow,
    MentalModelDomainRow, MentalModelPatternRow, MentalModelVocabularyRow,
};
pub use schema::safety::SafetyConfigRow;
pub use schema::wasm::WasmBinaryRow;
