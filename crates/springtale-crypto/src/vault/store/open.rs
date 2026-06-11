use std::collections::HashMap;
use std::path::PathBuf;

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};

use super::Vault;
use crate::error::CryptoError;
use crate::vault::{duress, kdf};

impl Vault {
    /// Open an existing vault file and decrypt it.
    ///
    /// Automatically detects dual-region (duress) vaults by file size.
    /// For dual vaults, tries the passphrase against both regions.
    /// Returns `is_duress_session() == true` if the duress region was unlocked.
    pub fn open(path: impl Into<PathBuf>, passphrase: &[u8]) -> Result<Self, CryptoError> {
        let path = path.into();

        // Open the file FIRST, then check permissions on the file descriptor.
        // This eliminates the TOCTOU race between permission check and read.
        let mut file = std::fs::File::open(&path)?;

        #[cfg(unix)]
        check_fd_permissions(&file)?;

        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut data)?;

        // Detect dual-region (duress) vault by constant file size
        if duress::is_dual_vault(data.len()) {
            return open_dual_vault(path, &data, passphrase);
        }

        // Legacy single-region format
        open_single_vault(path, &data, passphrase)
    }
}

/// Open a dual-region (duress) vault.
fn open_dual_vault(path: PathBuf, data: &[u8], passphrase: &[u8]) -> Result<Vault, CryptoError> {
    let region_total = duress::REGION_HEADER_SIZE + duress::REGION_SIZE;
    let (entries, salt, key, session) = duress::open_dual_vault(data, passphrase)?;

    // Determine which region was active and preserve the other's raw bytes
    let (active_index, inactive_bytes) = match session {
        duress::VaultSession::Real => {
            // Region 0 decrypted -> preserve region 1
            (0u8, data[region_total..].to_vec())
        }
        duress::VaultSession::Duress => {
            // Region 1 decrypted -> preserve region 0
            (1u8, data[..region_total].to_vec())
        }
    };

    Ok(Vault {
        path,
        entries: Some(entries),
        encryption_key: Some(key),
        salt,
        session,
        inactive_region_bytes: Some(inactive_bytes),
        active_region_index: active_index,
    })
}

/// Open a legacy single-region vault.
fn open_single_vault(path: PathBuf, data: &[u8], passphrase: &[u8]) -> Result<Vault, CryptoError> {
    if data.len() < 16 + 24 + 16 {
        return Err(CryptoError::VaultDecryptionFailed);
    }

    let salt: [u8; 16] = data[..16]
        .try_into()
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    let nonce_bytes: [u8; 24] = data[16..40]
        .try_into()
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    let ciphertext = &data[40..];

    let key = kdf::derive_key(passphrase, &salt)?;
    let nonce = XNonce::from_slice(&nonce_bytes);

    let cipher = crate::secret_use::with_key32(&key, |k| XChaCha20Poly1305::new_from_slice(k))
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    // Decode the entry map. Accepts both the crypto-agile `VaultPlaintext`
    // envelope (validating AEAD/KDF tags — fail closed on downgrade/forward
    // version) and pre-envelope legacy vaults (bare entry map). The AEAD tag
    // above already authenticated the plaintext, so the legacy fallback is a
    // genuine old vault, not tampering; it re-saves in the new format on the
    // next `save()`.
    let entries: HashMap<String, Vec<u8>> =
        crate::vault::plaintext::decode_region_entries(&plaintext)?;

    Ok(Vault {
        path,
        entries: Some(entries),
        encryption_key: Some(key),
        salt,
        session: duress::VaultSession::Real,
        inactive_region_bytes: None,
        active_region_index: 0,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Build a genuine pre-envelope (legacy) single-region vault —
    /// `[salt][nonce][XChaCha20Poly1305(serde_json(flat entry map))]`, the
    /// exact shape vaults had before the crypto-agile format change (the
    /// "unknown field `identity`" vault) — and confirm the current
    /// `Vault::open` reads it via the back-compat path.
    #[test]
    fn opens_legacy_flat_single_vault() {
        let passphrase = b"correct horse battery staple";
        let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
        entries.insert("identity".to_string(), b"keypair".to_vec());
        entries.insert("openai.api_key".to_string(), b"sk-test".to_vec());

        // Reproduce the OLD save path: plaintext = the bare entry map.
        let salt: [u8; 16] = [7; 16];
        let key = kdf::derive_key(passphrase, &salt).unwrap();
        let nonce_bytes: [u8; 24] = [9; 24];
        let nonce = XNonce::from_slice(&nonce_bytes);
        let cipher =
            crate::secret_use::with_key32(&key, |k| XChaCha20Poly1305::new_from_slice(k)).unwrap();
        let legacy_plaintext = serde_json::to_vec(&entries).unwrap();
        let ciphertext = cipher.encrypt(nonce, legacy_plaintext.as_ref()).unwrap();

        let mut file_data = Vec::new();
        file_data.extend_from_slice(&salt);
        file_data.extend_from_slice(&nonce_bytes);
        file_data.extend_from_slice(&ciphertext);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.vault");
        std::fs::write(&path, &file_data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }

        let vault = Vault::open(&path, passphrase).expect("legacy flat vault must open");
        assert_eq!(vault.get("identity").unwrap(), Some(&b"keypair".to_vec()));
        assert_eq!(
            vault.get("openai.api_key").unwrap(),
            Some(&b"sk-test".to_vec())
        );
    }
}

/// Check that an open vault file has secure permissions (0o600).
///
/// Uses fstat on the file descriptor to avoid TOCTOU race conditions --
/// the permission check operates on the same file handle we'll read from.
#[cfg(unix)]
fn check_fd_permissions(file: &std::fs::File) -> Result<(), CryptoError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    let mode = metadata.mode() & 0o777;
    if mode & 0o077 != 0 {
        tracing::warn!(
            mode = format!("{mode:04o}"),
            "vault file has insecure permissions (should be 0600)"
        );
        return Err(CryptoError::InsecurePermissions);
    }
    Ok(())
}
