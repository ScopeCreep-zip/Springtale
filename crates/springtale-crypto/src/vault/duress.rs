use std::collections::HashMap;
use std::path::Path;

use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit},
};
use rand::RngCore;
use secrecy::ExposeSecret;

use super::kdf;
use crate::error::CryptoError;

/// Which vault region was unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultSession {
    /// Real passphrase — full access to all data.
    Real,
    /// Duress passphrase — decoy profile with minimal data.
    Duress,
}

/// Constant size for each encrypted region (64 KiB).
///
/// Both real and decoy regions are padded to this size with random bytes.
/// An observer cannot tell which region contains more data — the file
/// size is always `2 * (16 + 24 + REGION_SIZE)` bytes.
pub const REGION_SIZE: usize = 65_536;

/// Size of the salt + nonce header per region.
pub const REGION_HEADER_SIZE: usize = 16 + 24; // salt + nonce

/// Total file size for a dual-region vault.
pub const DUAL_VAULT_FILE_SIZE: usize = 2 * (REGION_HEADER_SIZE + REGION_SIZE);

/// Result of opening a dual vault region: (entries, salt, derived_key, session).
pub type DualVaultOpenResult = (HashMap<String, Vec<u8>>, [u8; 16], secrecy::SecretBox<[u8; 32]>, VaultSession);

/// Result of decrypting a single region: (entries, salt, derived_key).
type RegionDecryptResult = (HashMap<String, Vec<u8>>, [u8; 16], secrecy::SecretBox<[u8; 32]>);

/// Create a dual-region vault file with real and decoy data.
///
/// Both regions are encrypted with different Argon2id-derived keys
/// and padded to exactly REGION_SIZE. The file size is constant —
/// an observer cannot tell which region has more data.
pub fn create_dual_vault(
    path: &Path,
    real_passphrase: &[u8],
    duress_passphrase: &[u8],
    real_entries: &HashMap<String, Vec<u8>>,
    decoy_entries: &HashMap<String, Vec<u8>>,
) -> Result<(), CryptoError> {
    let real_region = encrypt_region(real_passphrase, real_entries)?;
    let decoy_region = encrypt_region(duress_passphrase, decoy_entries)?;

    // File = [real_region (REGION_HEADER_SIZE + REGION_SIZE)]
    //        [decoy_region (REGION_HEADER_SIZE + REGION_SIZE)]
    let mut file_data = Vec::with_capacity(DUAL_VAULT_FILE_SIZE);
    file_data.extend_from_slice(&real_region);
    file_data.extend_from_slice(&decoy_region);

    debug_assert_eq!(file_data.len(), DUAL_VAULT_FILE_SIZE);

    // Write atomically
    let tmp_path = path.with_extension("bin.tmp");
    std::fs::write(&tmp_path, &file_data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&tmp_path, perms)?;
    }

    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Open a dual-region vault. Tries the passphrase against both regions.
///
/// Returns the decrypted entries and which session was unlocked.
/// If neither region decrypts, returns `VaultDecryptionFailed`.
pub fn open_dual_vault(data: &[u8], passphrase: &[u8]) -> Result<DualVaultOpenResult, CryptoError> {
    if data.len() != DUAL_VAULT_FILE_SIZE {
        return Err(CryptoError::VaultDecryptionFailed);
    }

    let real_region = &data[..REGION_HEADER_SIZE + REGION_SIZE];
    let decoy_region = &data[REGION_HEADER_SIZE + REGION_SIZE..];

    // Try real region first
    if let Ok((entries, salt, key)) = decrypt_region(real_region, passphrase) {
        return Ok((entries, salt, key, VaultSession::Real));
    }

    // Try decoy region
    if let Ok((entries, salt, key)) = decrypt_region(decoy_region, passphrase) {
        return Ok((entries, salt, key, VaultSession::Duress));
    }

    Err(CryptoError::VaultDecryptionFailed)
}

/// Check if a file is a dual-region vault (by constant file size).
pub fn is_dual_vault(data_len: usize) -> bool {
    data_len == DUAL_VAULT_FILE_SIZE
}

/// Encrypt entries into a fixed-size region.
///
/// Format: [salt (16)] [nonce (24)] [padded_ciphertext (REGION_SIZE)]
///
/// The plaintext is JSON-serialized entries, padded with random bytes
/// to fill REGION_SIZE after encryption (accounting for AEAD tag).
fn encrypt_region(
    passphrase: &[u8],
    entries: &HashMap<String, Vec<u8>>,
) -> Result<Vec<u8>, CryptoError> {
    let salt = kdf::generate_salt();
    let key = kdf::derive_key(passphrase, &salt)?;

    let plaintext =
        serde_json::to_vec(entries).map_err(|e| CryptoError::Serialization(e.to_string()))?;

    // AEAD tag is 16 bytes for Poly1305
    const TAG_SIZE: usize = 16;
    let max_plaintext_size = REGION_SIZE - TAG_SIZE;

    if plaintext.len() > max_plaintext_size {
        return Err(CryptoError::Serialization(
            "vault data too large for region".into(),
        ));
    }

    // Pad plaintext with random bytes to constant size (before encryption)
    let mut padded_plaintext = vec![0u8; max_plaintext_size];
    padded_plaintext[..plaintext.len()].copy_from_slice(&plaintext);
    // Fill padding with random bytes so ciphertext looks uniform
    rand::thread_rng().fill_bytes(&mut padded_plaintext[plaintext.len()..]);
    // Store actual length at the end (last 8 bytes of padded region)
    // so we can extract the real data after decryption
    let len_offset = max_plaintext_size - 8;
    padded_plaintext[len_offset..].copy_from_slice(&(plaintext.len() as u64).to_le_bytes());

    let nonce = XChaCha20Poly1305::generate_nonce(&mut rand::rngs::OsRng);
    // SECURITY: expose needed for AEAD encryption of vault region
    let cipher = XChaCha20Poly1305::new_from_slice(key.expose_secret())
        .map_err(|_| CryptoError::KeyGeneration("invalid key length".into()))?;

    let ciphertext = cipher
        .encrypt(&nonce, padded_plaintext.as_slice())
        .map_err(|_| CryptoError::KeyGeneration("region encryption failed".into()))?;

    debug_assert_eq!(ciphertext.len(), REGION_SIZE);

    let mut region = Vec::with_capacity(REGION_HEADER_SIZE + REGION_SIZE);
    region.extend_from_slice(&salt);
    region.extend_from_slice(nonce.as_slice());
    region.extend_from_slice(&ciphertext);

    Ok(region)
}

/// Encrypt entries into a fixed-size region using an EXISTING key and salt.
///
/// Used by `Vault::save()` for asymmetric re-encryption — the vault
/// already has the derived key in memory (no passphrase needed).
pub fn encrypt_region_with_key(
    key: &[u8; 32],
    salt: &[u8; 16],
    entries: &HashMap<String, Vec<u8>>,
) -> Result<Vec<u8>, CryptoError> {
    let plaintext =
        serde_json::to_vec(entries).map_err(|e| CryptoError::Serialization(e.to_string()))?;

    const TAG_SIZE: usize = 16;
    let max_plaintext_size = REGION_SIZE - TAG_SIZE;

    if plaintext.len() > max_plaintext_size {
        return Err(CryptoError::Serialization(
            "vault data too large for region".into(),
        ));
    }

    let mut padded_plaintext = vec![0u8; max_plaintext_size];
    padded_plaintext[..plaintext.len()].copy_from_slice(&plaintext);
    rand::thread_rng().fill_bytes(&mut padded_plaintext[plaintext.len()..]);
    let len_offset = max_plaintext_size - 8;
    padded_plaintext[len_offset..].copy_from_slice(&(plaintext.len() as u64).to_le_bytes());

    let nonce = XChaCha20Poly1305::generate_nonce(&mut rand::rngs::OsRng);
    // SECURITY: key used for AEAD encryption of vault region
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| CryptoError::KeyGeneration("invalid key length".into()))?;

    let ciphertext = cipher
        .encrypt(&nonce, padded_plaintext.as_slice())
        .map_err(|_| CryptoError::KeyGeneration("region encryption failed".into()))?;

    debug_assert_eq!(ciphertext.len(), REGION_SIZE);

    let mut region = Vec::with_capacity(REGION_HEADER_SIZE + REGION_SIZE);
    region.extend_from_slice(salt);
    region.extend_from_slice(nonce.as_slice());
    region.extend_from_slice(&ciphertext);

    Ok(region)
}

/// Decrypt a region and extract the original entries.
///
/// Returns (entries, salt, key) on success.
fn decrypt_region(region: &[u8], passphrase: &[u8]) -> Result<RegionDecryptResult, CryptoError> {
    if region.len() != REGION_HEADER_SIZE + REGION_SIZE {
        return Err(CryptoError::VaultDecryptionFailed);
    }

    let salt: [u8; 16] = region[..16]
        .try_into()
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    let nonce_bytes: [u8; 24] = region[16..40]
        .try_into()
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    let ciphertext = &region[40..];

    let key = kdf::derive_key(passphrase, &salt)?;
    let nonce = chacha20poly1305::XNonce::from_slice(&nonce_bytes);

    // SECURITY: expose needed for AEAD decryption of vault region
    let cipher = XChaCha20Poly1305::new_from_slice(key.expose_secret())
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    let padded_plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    // Extract actual data length from last 8 bytes
    const TAG_SIZE: usize = 16;
    let max_plaintext_size = REGION_SIZE - TAG_SIZE;
    let len_offset = max_plaintext_size - 8;
    let len_bytes: [u8; 8] = padded_plaintext[len_offset..len_offset + 8]
        .try_into()
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    let actual_len = u64::from_le_bytes(len_bytes) as usize;

    if actual_len > len_offset {
        return Err(CryptoError::VaultDecryptionFailed);
    }

    let entries: HashMap<String, Vec<u8>> = serde_json::from_slice(&padded_plaintext[..actual_len])
        .map_err(|e| CryptoError::Serialization(e.to_string()))?;

    Ok((entries, salt, key))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_dual_vault_real_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dual_vault.bin");

        let mut real = HashMap::new();
        real.insert("secret".into(), b"real_data".to_vec());

        let mut decoy = HashMap::new();
        decoy.insert("note".into(), b"shopping list".to_vec());

        create_dual_vault(&path, b"real_pass", b"duress_pass", &real, &decoy).unwrap();

        let data = std::fs::read(&path).unwrap();
        let (entries, _, _key, session) = open_dual_vault(&data, b"real_pass").unwrap();

        assert_eq!(session, VaultSession::Real);
        assert_eq!(entries.get("secret").unwrap(), b"real_data");
    }

    #[test]
    fn test_dual_vault_duress_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dual_vault.bin");

        let mut real = HashMap::new();
        real.insert("secret".into(), b"real_data".to_vec());

        let mut decoy = HashMap::new();
        decoy.insert("note".into(), b"shopping list".to_vec());

        create_dual_vault(&path, b"real_pass", b"duress_pass", &real, &decoy).unwrap();

        let data = std::fs::read(&path).unwrap();
        let (entries, _, _key, session) = open_dual_vault(&data, b"duress_pass").unwrap();

        assert_eq!(session, VaultSession::Duress);
        assert_eq!(entries.get("note").unwrap(), b"shopping list");
        assert!(entries.get("secret").is_none()); // Real data NOT visible
    }

    #[test]
    fn test_dual_vault_wrong_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dual_vault.bin");

        create_dual_vault(&path, b"real", b"duress", &HashMap::new(), &HashMap::new()).unwrap();

        let data = std::fs::read(&path).unwrap();
        let result = open_dual_vault(&data, b"wrong");
        assert!(result.is_err());
    }

    #[test]
    fn test_dual_vault_constant_file_size() {
        let dir = tempfile::tempdir().unwrap();

        // Vault with lots of real data
        let path1 = dir.path().join("big.bin");
        let mut big = HashMap::new();
        for i in 0..100 {
            big.insert(format!("key_{i}"), vec![0xAA; 100]);
        }
        create_dual_vault(&path1, b"r", b"d", &big, &HashMap::new()).unwrap();

        // Vault with minimal data
        let path2 = dir.path().join("small.bin");
        create_dual_vault(&path2, b"r", b"d", &HashMap::new(), &HashMap::new()).unwrap();

        let size1 = std::fs::metadata(&path1).unwrap().len();
        let size2 = std::fs::metadata(&path2).unwrap().len();

        assert_eq!(size1, size2);
        assert_eq!(size1, DUAL_VAULT_FILE_SIZE as u64);
    }

    #[test]
    fn test_is_dual_vault_detection() {
        assert!(is_dual_vault(DUAL_VAULT_FILE_SIZE));
        assert!(!is_dual_vault(100));
        assert!(!is_dual_vault(0));
    }
}
