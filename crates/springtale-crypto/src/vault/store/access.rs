use super::Vault;
use crate::error::CryptoError;

impl Vault {
    /// Store a value in the vault.
    pub fn set(&mut self, key: impl Into<String>, value: Vec<u8>) -> Result<(), CryptoError> {
        let entries = self.entries.as_mut().ok_or(CryptoError::VaultLocked)?;
        entries.insert(key.into(), value);
        Ok(())
    }

    /// Retrieve a value from the vault.
    pub fn get(&self, key: &str) -> Result<Option<&Vec<u8>>, CryptoError> {
        let entries = self.entries.as_ref().ok_or(CryptoError::VaultLocked)?;
        Ok(entries.get(key))
    }

    /// Remove a value from the vault.
    pub fn remove(&mut self, key: &str) -> Result<Option<Vec<u8>>, CryptoError> {
        let entries = self.entries.as_mut().ok_or(CryptoError::VaultLocked)?;
        Ok(entries.remove(key))
    }

    /// List all keys in the vault.
    pub fn keys(&self) -> Result<Vec<&String>, CryptoError> {
        let entries = self.entries.as_ref().ok_or(CryptoError::VaultLocked)?;
        Ok(entries.keys().collect())
    }
}
