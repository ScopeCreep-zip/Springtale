//! Operation errors for the shared runtime.

use thiserror::Error;

/// Errors from shared operations (rules, connectors, formations, etc.).
///
/// Each variant has a stable error ID (e.g., `E001`) for the `springtale fix` command.
#[derive(Debug, Error)]
pub enum OperationError {
    #[error("[E001] store error: {0}")]
    Store(#[from] springtale_store::StoreError),

    #[error("[E002] rule error: {0}")]
    Rule(String),

    #[error("[E003] connector error: {0}")]
    Connector(String),

    #[error("[E004] formation error: {0}")]
    Formation(String),

    #[error("[E005] not found: {0}")]
    NotFound(String),

    #[error("[E006] validation error: {0}")]
    Validation(String),

    #[error("[E007] serialization error: {0}")]
    Serialization(String),

    #[error("[E008] AI error: {0}")]
    Ai(String),

    #[error("[E009] initialization failed: {0}")]
    Init(String),
}

impl OperationError {
    /// Stable error ID for each variant.
    pub fn error_id(&self) -> &'static str {
        match self {
            Self::Store(_) => "E001",
            Self::Rule(_) => "E002",
            Self::Connector(_) => "E003",
            Self::Formation(_) => "E004",
            Self::NotFound(_) => "E005",
            Self::Validation(_) => "E006",
            Self::Serialization(_) => "E007",
            Self::Ai(_) => "E008",
            Self::Init(_) => "E009",
        }
    }
}
