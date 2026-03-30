use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("connector not found: {0}")]
    NotFound(String),

    #[error("manifest validation failed: {0}")]
    ManifestInvalid(String),

    #[error("manifest signature verification failed")]
    SignatureInvalid,

    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    #[error("capability requires user approval: {0}")]
    RequiresApproval(String),

    #[error("WASM sandbox error: {0}")]
    Sandbox(String),

    #[error("WASM fuel exhausted after {used} instructions (limit: {limit})")]
    FuelExhausted { used: u64, limit: u64 },

    #[error("WASM memory limit exceeded")]
    MemoryLimitExceeded,

    #[error("WASM binary hash mismatch")]
    WasmHashMismatch,

    #[error("connector execution failed: {0}")]
    ExecutionFailed(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("crypto error: {0}")]
    Crypto(#[from] springtale_crypto::error::CryptoError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
