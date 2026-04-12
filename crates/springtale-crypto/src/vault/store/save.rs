use std::path::Path;

use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit},
};
use secrecy::ExposeSecret;

use super::Vault;
use crate::error::CryptoError;
use crate::vault::duress;

impl Vault {
    /// Save the vault to disk (encrypted).
    ///
    /// No-op for ephemeral vaults -- data stays in memory only.
    ///
    /// For dual (duress) vaults: re-encrypts ONLY the active region.
    /// The inactive region's raw ciphertext is preserved unchanged
    /// (asymmetric save -- VeraCrypt model).
    pub fn save(&self) -> Result<(), CryptoError> {
        if self.is_ephemeral() {
            return Ok(());
        }
        let entries = self.entries.as_ref().ok_or(CryptoError::VaultLocked)?;
        let key_box = self
            .encryption_key
            .as_ref()
            .ok_or(CryptoError::VaultLocked)?;

        // SECURITY: expose needed for AEAD encryption
        let key = key_box.expose_secret();

        let file_data = if let Some(ref inactive_bytes) = self.inactive_region_bytes {
            // Dual vault: re-encrypt active region, preserve inactive region
            let active_region = duress::encrypt_region_with_key(key, &self.salt, entries)?;

            let mut data = Vec::with_capacity(duress::DUAL_VAULT_FILE_SIZE);
            if self.active_region_index == 0 {
                data.extend_from_slice(&active_region);
                data.extend_from_slice(inactive_bytes);
            } else {
                data.extend_from_slice(inactive_bytes);
                data.extend_from_slice(&active_region);
            }
            data
        } else {
            // Legacy single-region format
            let plaintext = serde_json::to_vec(entries)
                .map_err(|e| CryptoError::Serialization(e.to_string()))?;

            let nonce = XChaCha20Poly1305::generate_nonce(&mut rand::rngs::OsRng);
            let cipher = XChaCha20Poly1305::new_from_slice(key)
                .map_err(|_| CryptoError::KeyGeneration("invalid key length".into()))?;

            let ciphertext = cipher
                .encrypt(&nonce, plaintext.as_slice())
                .map_err(|_| CryptoError::KeyGeneration("encryption failed".into()))?;

            let mut data = Vec::with_capacity(16 + 24 + ciphertext.len());
            data.extend_from_slice(&self.salt);
            data.extend_from_slice(nonce.as_slice());
            data.extend_from_slice(&ciphertext);
            data
        };

        // Write atomically: write to temp file then rename
        let tmp_path = self.path.with_extension("bin.tmp");
        std::fs::write(&tmp_path, &file_data)?;

        #[cfg(unix)]
        set_permissions(&tmp_path)?;

        std::fs::rename(&tmp_path, &self.path)?;

        Ok(())
    }
}

/// Set file permissions to 0o600 (owner read/write only).
#[cfg(unix)]
fn set_permissions(path: &Path) -> Result<(), CryptoError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}
