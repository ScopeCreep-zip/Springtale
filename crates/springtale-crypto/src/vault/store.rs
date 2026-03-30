use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, AeadCore, KeyInit},
};
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroize;

use super::kdf;
use crate::error::CryptoError;

/// Encrypted key-value store for secrets at rest.
///
/// The vault file has no magic bytes or headers — it is indistinguishable
/// from random data without the correct passphrase. Extension is `.bin`.
///
/// Internal format (after decryption):
/// - 16 bytes: salt (for key derivation)
/// - 24 bytes: nonce (for XChaCha20-Poly1305)
/// - remainder: ciphertext (encrypted JSON map of key -> base64-encoded value)
///
/// On disk: `[salt (16)] [nonce (24)] [ciphertext ...]`
pub struct Vault {
    path: PathBuf,
    /// The decrypted key-value entries (in memory while unlocked).
    entries: Option<HashMap<String, Vec<u8>>>,
    /// The derived encryption key (in memory while unlocked).
    encryption_key: Option<SecretBox<[u8; 32]>>,
    /// The salt used for this vault file.
    salt: [u8; 16],
}

impl Vault {
    /// Create a new vault at the given path.
    ///
    /// Does not write to disk until `save()` is called.
    pub fn create(path: impl Into<PathBuf>, passphrase: &[u8]) -> Result<Self, CryptoError> {
        let path = path.into();
        let salt = kdf::generate_salt();
        let mut key = kdf::derive_key(passphrase, &salt)?;

        let vault = Self {
            path,
            entries: Some(HashMap::new()),
            encryption_key: Some(SecretBox::new(Box::new(key))),
            salt,
        };

        key.zeroize();
        Ok(vault)
    }

    /// Open an existing vault file and decrypt it.
    pub fn open(path: impl Into<PathBuf>, passphrase: &[u8]) -> Result<Self, CryptoError> {
        let path = path.into();

        // Open the file FIRST, then check permissions on the file descriptor.
        // This eliminates the TOCTOU race between permission check and read.
        let mut file = std::fs::File::open(&path)?;

        #[cfg(unix)]
        check_fd_permissions(&file)?;

        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut data)?;

        if data.len() < 16 + 24 + 16 {
            // salt + nonce + minimum ciphertext (at least a tag)
            return Err(CryptoError::VaultDecryptionFailed);
        }

        let salt: [u8; 16] = data[..16]
            .try_into()
            .map_err(|_| CryptoError::VaultDecryptionFailed)?;
        let nonce_bytes: [u8; 24] = data[16..40]
            .try_into()
            .map_err(|_| CryptoError::VaultDecryptionFailed)?;
        let ciphertext = &data[40..];

        let mut key = kdf::derive_key(passphrase, &salt)?;
        let nonce = XNonce::from_slice(&nonce_bytes);

        // SECURITY: expose needed for AEAD decryption
        let cipher = XChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| CryptoError::VaultDecryptionFailed)?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::VaultDecryptionFailed)?;

        let entries: HashMap<String, Vec<u8>> = serde_json::from_slice(&plaintext)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;

        let vault = Self {
            path,
            entries: Some(entries),
            encryption_key: Some(SecretBox::new(Box::new(key))),
            salt,
        };

        key.zeroize();
        Ok(vault)
    }

    /// Save the vault to disk (encrypted).
    pub fn save(&self) -> Result<(), CryptoError> {
        let entries = self.entries.as_ref().ok_or(CryptoError::VaultLocked)?;
        let key_box = self
            .encryption_key
            .as_ref()
            .ok_or(CryptoError::VaultLocked)?;

        let plaintext =
            serde_json::to_vec(entries).map_err(|e| CryptoError::Serialization(e.to_string()))?;

        // Generate a fresh nonce for each save (192-bit, collision negligible at 2^-96)
        let nonce = XChaCha20Poly1305::generate_nonce(&mut rand::rngs::OsRng);

        // SECURITY: expose needed for AEAD encryption
        let key = key_box.expose_secret();
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| CryptoError::KeyGeneration("invalid key length".into()))?;

        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_slice())
            .map_err(|_| CryptoError::KeyGeneration("encryption failed".into()))?;

        // Write: [salt (16)] [nonce (24)] [ciphertext]
        let mut file_data = Vec::with_capacity(16 + 24 + ciphertext.len());
        file_data.extend_from_slice(&self.salt);
        file_data.extend_from_slice(nonce.as_slice());
        file_data.extend_from_slice(&ciphertext);

        // Write atomically: write to temp file then rename
        let tmp_path = self.path.with_extension("bin.tmp");
        std::fs::write(&tmp_path, &file_data)?;

        #[cfg(unix)]
        set_permissions(&tmp_path)?;

        std::fs::rename(&tmp_path, &self.path)?;

        Ok(())
    }

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

    /// Lock the vault — zeroize the encryption key and clear entries.
    pub fn lock(&mut self) {
        self.entries = None;
        self.encryption_key = None;
        // SecretBox handles zeroizing the key on drop
    }

    /// Check if the vault is currently unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.entries.is_some()
    }

    /// Get the vault file path.
    pub fn path(&self) -> &Path {
        &self.path
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

/// Check that an open vault file has secure permissions (0o600).
///
/// Uses fstat on the file descriptor to avoid TOCTOU race conditions —
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_vault_path() -> PathBuf {
        let dir = std::env::temp_dir().join("springtale_test_vaults");
        fs::create_dir_all(&dir).ok();
        dir.join(format!("vault_{}.bin", uuid::Uuid::new_v4()))
    }

    #[test]
    fn test_create_and_save() {
        let path = temp_vault_path();
        let vault = Vault::create(&path, b"testpass").unwrap();
        vault.save().unwrap();
        assert!(path.exists());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_create_set_save_open_get() {
        let path = temp_vault_path();

        // Create, set a value, save
        let mut vault = Vault::create(&path, b"testpass").unwrap();
        vault.set("my_key", b"my_secret_value".to_vec()).unwrap();
        vault.save().unwrap();

        // Open with correct passphrase, get the value
        let vault2 = Vault::open(&path, b"testpass").unwrap();
        let val = vault2.get("my_key").unwrap();
        assert_eq!(
            val.map(|v| v.as_slice()),
            Some(b"my_secret_value".as_slice())
        );

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let path = temp_vault_path();

        let vault = Vault::create(&path, b"correct").unwrap();
        vault.save().unwrap();

        let result = Vault::open(&path, b"wrong");
        assert!(result.is_err());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_lock_prevents_access() {
        let path = temp_vault_path();
        let mut vault = Vault::create(&path, b"testpass").unwrap();

        assert!(vault.is_unlocked());
        vault.lock();
        assert!(!vault.is_unlocked());

        let result = vault.get("anything");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_key() {
        let path = temp_vault_path();
        let mut vault = Vault::create(&path, b"testpass").unwrap();

        vault.set("key1", b"val1".to_vec()).unwrap();
        let removed = vault.remove("key1").unwrap();
        assert_eq!(removed.as_deref(), Some(b"val1".as_slice()));

        let get_after = vault.get("key1").unwrap();
        assert!(get_after.is_none());
    }

    #[test]
    fn test_keypair_persist_through_vault() {
        use crate::identity::keypair::Keypair;

        let path = temp_vault_path();
        let mut vault = Vault::create(&path, b"testpass").unwrap();

        // Generate a keypair and store it in the vault
        let keypair = Keypair::generate().unwrap();
        let node_id = keypair.node_id();
        let secret_bytes = *keypair.expose_secret_bytes();
        vault.set("identity", secret_bytes.to_vec()).unwrap();
        vault.save().unwrap();

        // Open the vault and reconstruct the keypair
        let vault2 = Vault::open(&path, b"testpass").unwrap();
        let stored = vault2.get("identity").unwrap().cloned().unwrap();
        let restored_bytes: [u8; 32] = stored.try_into().ok().expect("stored bytes should be 32");
        let restored = Keypair::from_secret_bytes(restored_bytes).unwrap();

        // Same identity
        assert_eq!(node_id, restored.node_id());

        // Sign with restored key, verify with original public key
        let msg = b"round-trip test";
        let sig = restored.sign(msg);
        use ed25519_dalek::Verifier;
        keypair.verifying_key().verify(msg, &sig).unwrap();

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_save_produces_different_nonces() {
        let path = temp_vault_path();
        let vault = Vault::create(&path, b"testpass").unwrap();

        // Save twice, read the nonce bytes (offset 16..40) each time
        vault.save().unwrap();
        let data1 = fs::read(&path).unwrap();
        let nonce1 = &data1[16..40];

        vault.save().unwrap();
        let data2 = fs::read(&path).unwrap();
        let nonce2 = &data2[16..40];

        // Nonces must differ (fresh random per save)
        assert_ne!(nonce1, nonce2);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_vault_file_has_no_magic_bytes() {
        let path = temp_vault_path();
        let vault = Vault::create(&path, b"testpass").unwrap();
        vault.save().unwrap();

        // First bytes are the salt (random), not a recognizable header
        let data = fs::read(&path).unwrap();
        assert!(!data.starts_with(b"PK")); // zip
        assert!(!data.starts_with(b"\x89PNG")); // png
        assert!(!data.starts_with(b"SQLite")); // sqlite
        assert!(!data.starts_with(b"{")); // json

        fs::remove_file(&path).ok();
    }
}
