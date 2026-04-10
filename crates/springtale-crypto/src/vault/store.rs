use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, AeadCore, KeyInit},
};
use secrecy::{ExposeSecret, SecretBox};

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
    /// Which vault session is active (Real or Duress).
    session: super::duress::VaultSession,
    /// For dual vaults: raw bytes of the INACTIVE region (preserved unchanged on save).
    /// None for legacy single-region vaults.
    inactive_region_bytes: Option<Vec<u8>>,
    /// For dual vaults: which region index (0 or 1) is the active one.
    active_region_index: u8,
}

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
            session: super::duress::VaultSession::Real,
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
            session: super::duress::VaultSession::Real,
            inactive_region_bytes: None,
            active_region_index: 0,
        })
    }

    /// Returns true if this vault is ephemeral (no file backing).
    pub fn is_ephemeral(&self) -> bool {
        self.path.as_os_str().is_empty()
    }

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
        if super::duress::is_dual_vault(data.len()) {
            let region_total = super::duress::REGION_HEADER_SIZE + super::duress::REGION_SIZE;
            let (entries, salt, key, session) =
                super::duress::open_dual_vault(&data, passphrase)?;

            // Determine which region was active and preserve the other's raw bytes
            let (active_index, inactive_bytes) = match session {
                super::duress::VaultSession::Real => {
                    // Region 0 decrypted → preserve region 1
                    (0u8, data[region_total..].to_vec())
                }
                super::duress::VaultSession::Duress => {
                    // Region 1 decrypted → preserve region 0
                    (1u8, data[..region_total].to_vec())
                }
            };

            return Ok(Self {
                path,
                entries: Some(entries),
                encryption_key: Some(key),
                salt,
                session,
                inactive_region_bytes: Some(inactive_bytes),
                active_region_index: active_index,
            });
        }

        // Legacy single-region format
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

        Ok(Self {
            path,
            entries: Some(entries),
            encryption_key: Some(key),
            salt,
            session: super::duress::VaultSession::Real,
            inactive_region_bytes: None,
            active_region_index: 0,
        })
    }

    /// Save the vault to disk (encrypted).
    ///
    /// No-op for ephemeral vaults — data stays in memory only.
    ///
    /// For dual (duress) vaults: re-encrypts ONLY the active region.
    /// The inactive region's raw ciphertext is preserved unchanged
    /// (asymmetric save — VeraCrypt model).
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
            let active_region = super::duress::encrypt_region_with_key(key, &self.salt, entries)?;

            let mut data = Vec::with_capacity(super::duress::DUAL_VAULT_FILE_SIZE);
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

    /// Check if this session was unlocked with a duress passphrase.
    pub fn is_duress_session(&self) -> bool {
        self.session == super::duress::VaultSession::Duress
    }

    /// Get the vault session type.
    pub fn session(&self) -> super::duress::VaultSession {
        self.session
    }

    /// Emergency data destruction — zeroes key material and overwrites vault file.
    ///
    /// Must complete within 3 seconds. Synchronous (no async overhead).
    ///
    /// 1. Lock vault (zeroes encryption key in memory via SecretBox drop)
    /// 2. Overwrite vault file with random bytes
    /// 3. Delete vault file
    ///
    /// After this call, the vault is irrecoverable.
    pub fn panic_wipe(&mut self) -> Result<(), CryptoError> {
        // Step 1: Zero key material in memory
        self.lock();

        // Step 2+3: Overwrite and delete vault file (skip for ephemeral)
        if !self.is_ephemeral() {
            super::wipe::wipe_vault_file(&self.path)?;
        }

        Ok(())
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

    #[test]
    fn test_dual_vault_open_modify_save_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dual.bin");

        // Create dual vault with real + decoy data
        let mut real = HashMap::new();
        real.insert("identity".into(), b"real_key".to_vec());
        let mut decoy = HashMap::new();
        decoy.insert("note".into(), b"groceries".to_vec());

        crate::vault::duress::create_dual_vault(&path, b"real_pass", b"duress_pass", &real, &decoy)
            .unwrap();

        // Open with real passphrase, modify, save
        let mut vault = Vault::open(&path, b"real_pass").unwrap();
        assert!(!vault.is_duress_session());
        assert_eq!(
            vault.get("identity").unwrap().unwrap().as_slice(),
            b"real_key"
        );

        vault.set("new_key", b"new_value".to_vec()).unwrap();
        vault.save().unwrap();

        // Reopen with real passphrase — new data should be there
        let vault2 = Vault::open(&path, b"real_pass").unwrap();
        assert_eq!(
            vault2.get("new_key").unwrap().unwrap().as_slice(),
            b"new_value"
        );
        assert_eq!(
            vault2.get("identity").unwrap().unwrap().as_slice(),
            b"real_key"
        );

        // Reopen with duress passphrase — decoy data should still be intact
        let vault3 = Vault::open(&path, b"duress_pass").unwrap();
        assert!(vault3.is_duress_session());
        assert_eq!(
            vault3.get("note").unwrap().unwrap().as_slice(),
            b"groceries"
        );
        assert!(vault3.get("new_key").unwrap().is_none()); // Real data NOT visible

        // File size should still be constant
        let file_size = fs::metadata(&path).unwrap().len();
        assert_eq!(file_size, crate::vault::duress::DUAL_VAULT_FILE_SIZE as u64);
    }

    #[test]
    fn test_legacy_vault_still_works_with_duress_code() {
        let path = temp_vault_path();

        // Create a legacy single-region vault
        let mut vault = Vault::create(&path, b"legacy_pass").unwrap();
        vault.set("key1", b"val1".to_vec()).unwrap();
        vault.save().unwrap();

        // Open it — should work, session = Real, not duress
        let vault2 = Vault::open(&path, b"legacy_pass").unwrap();
        assert!(!vault2.is_duress_session());
        assert_eq!(vault2.get("key1").unwrap().unwrap().as_slice(), b"val1");

        // File size should NOT be the dual vault constant
        let file_size = fs::metadata(&path).unwrap().len();
        assert_ne!(file_size, crate::vault::duress::DUAL_VAULT_FILE_SIZE as u64);

        fs::remove_file(&path).ok();
    }
}
