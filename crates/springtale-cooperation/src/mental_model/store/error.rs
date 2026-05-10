use thiserror::Error;

/// Narrow error type for the mental-model store. Distinct from the
/// cooperation-level aggregate so callers can pattern-match on specific
/// persistence failures without pulling in the whole cooperation surface.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("COOP-D001: backend error: {0}")]
    Backend(#[from] springtale_store::StoreError),
    #[error("COOP-D002: serialization error: {0}")]
    Serialization(String),
    #[error("COOP-D003: invalid row data: {0}")]
    InvalidRow(String),
}
