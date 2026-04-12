use std::collections::HashMap;
use std::path::PathBuf;

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use secrecy::ExposeSecret;

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

    // SECURITY: expose needed for AEAD decryption
    let cipher = XChaCha20Poly1305::new_from_slice(key.expose_secret())
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    let entries: HashMap<String, Vec<u8>> = serde_json::from_slice(&plaintext)
        .map_err(|e| CryptoError::Serialization(e.to_string()))?;

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
