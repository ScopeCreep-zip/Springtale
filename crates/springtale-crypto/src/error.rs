use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("vault is locked")]
    VaultLocked,

    #[error("vault decryption failed — wrong passphrase or corrupted data")]
    VaultDecryptionFailed,

    #[error("invalid signature")]
    InvalidSignature,

    #[error("key generation failed: {0}")]
    KeyGeneration(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("vault I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("key not found: {0}")]
    KeyNotFound(String),

    #[error("vault file has insecure permissions")]
    InsecurePermissions,
}
