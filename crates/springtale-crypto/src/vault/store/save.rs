use std::path::Path;

use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit},
};

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

        let file_data = if let Some(ref inactive_bytes) = self.inactive_region_bytes {
            // Dual vault: re-encrypt active region, preserve inactive region
            let active_region = crate::secret_use::with_key32(key_box, |key| {
                duress::encrypt_region_with_key(key, &self.salt, entries)
            })?;

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
            // Single-region format with crypto-agile plaintext envelope:
            // entries get wrapped in `VaultPlaintext` carrying AEAD + KDF
            // tags and KDF params, all authenticated by the AEAD tag.
            let envelope = crate::vault::VaultPlaintext::with_defaults(entries.clone());
            let plaintext = serde_json::to_vec(&envelope)
                .map_err(|e| CryptoError::Serialization(e.to_string()))?;

            let nonce = XChaCha20Poly1305::generate_nonce(&mut rand::rngs::OsRng);
            let cipher =
                crate::secret_use::with_key32(key_box, |k| XChaCha20Poly1305::new_from_slice(k))
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

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! Whole-codebase audit Finding #3 — assert the vault file lands
    //! on disk with mode 0o600. The set_permissions function exists
    //! in 4 places (this file, vault/backup.rs lines 70 + 175,
    //! vault/duress.rs:80) but until this test existed no invariant
    //! check stopped a future refactor from dropping the chmod.

    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use crate::vault::store::Vault;

    #[test]
    fn vault_file_persists_with_0o600_mode() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vault.bin");
        let passphrase = b"audit-test-passphrase";

        // Create + save a new vault via the production code path.
        let mut vault = Vault::create(&path, passphrase).unwrap();
        vault
            .set("test-key", b"test-value".to_vec())
            .unwrap();
        vault.save().unwrap();

        // Round-trip into the kernel: read the file's metadata and
        // assert the permission bits are exactly owner-rw, nothing
        // else. The mask is 0o777 because higher bits (setuid/setgid/
        // sticky) are not relevant to data files.
        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "vault file mode {mode:o} != 0o600 — chmod regression"
        );
    }

    #[test]
    fn resaved_vault_still_has_0o600_mode() {
        // The atomic save() writes to a `.tmp` sibling then renames.
        // On rename, the destination should inherit the tmp file's
        // chmod-after-write mode. Verify the second save preserves
        // 0o600 — catches a regression where set_permissions is
        // only applied on first-create.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vault.bin");
        let passphrase = b"audit-test-passphrase-2";

        let mut vault = Vault::create(&path, passphrase).unwrap();
        vault.set("k1", b"v1".to_vec()).unwrap();
        vault.save().unwrap();
        vault.set("k2", b"v2".to_vec()).unwrap();
        vault.save().unwrap();

        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "vault file mode {mode:o} != 0o600 after resave"
        );
    }
}
