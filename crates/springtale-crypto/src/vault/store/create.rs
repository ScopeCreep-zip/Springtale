use std::collections::HashMap;
use std::path::PathBuf;

use super::Vault;
use crate::error::CryptoError;
use crate::vault::kdf;

impl Vault {
    /// Create a new vault at the given path.
    ///
    /// Does not write to disk until `save()` is called.
    pub fn create(path: impl Into<PathBuf>, passphrase: &[u8]) -> Result<Self, CryptoError> {
        let path = path.into();
        let salt = kdf::generate_salt();
        let key = kdf::derive_key(passphrase, &salt)?;

        Ok(Self {
            path,
            entries: Some(HashMap::new()),
            encryption_key: Some(key),
            salt,
            session: super::super::duress::VaultSession::Real,
            inactive_region_bytes: None,
            active_region_index: 0,
        })
    }

    /// Create an ephemeral vault (memory-only, no file I/O).
    ///
    /// All state is lost on exit. `save()` is a no-op.
    /// Used with `--ephemeral` flag for travel mode / device seizure protection.
    pub fn create_ephemeral(passphrase: &[u8]) -> Result<Self, CryptoError> {
        let salt = kdf::generate_salt();
        let key = kdf::derive_key(passphrase, &salt)?;

        Ok(Self {
            path: PathBuf::new(), // empty path signals ephemeral
            entries: Some(HashMap::new()),
            encryption_key: Some(key),
            salt,
            session: super::super::duress::VaultSession::Real,
            inactive_region_bytes: None,
            active_region_index: 0,
        })
    }
}
