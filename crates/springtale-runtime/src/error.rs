//! Operation errors for the shared runtime.

use thiserror::Error;

/// Errors from shared operations (rules, connectors, formations, etc.).
#[derive(Debug, Error)]
pub enum OperationError {
    #[error("store error: {0}")]
    Store(#[from] springtale_store::StoreError),

    #[error("rule error: {0}")]
    Rule(String),

    #[error("connector error: {0}")]
    Connector(String),

    #[error("formation error: {0}")]
    Formation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}
