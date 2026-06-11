use std::path::Path;

use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit},
};

use super::kdf;
use crate::error::CryptoError;

/// Export an encrypted backup containing vault, database, and config files.
///
/// Backup format (indistinguishable from random data):
/// ```text
/// [salt (16)] [nonce (24)] [ciphertext of payload]
/// ```
///
/// Payload before encryption:
/// ```text
/// [vault_len (8 bytes LE)] [vault_bytes ...]
/// [db_len (8 bytes LE)]    [db_bytes ...]
/// [config_len (8 bytes LE)] [config_bytes ...]
/// ```
///
/// Travel passphrase → Argon2id → XChaCha20-Poly1305 (same crypto as vault).
pub fn export_backup(
    vault_path: &Path,
    db_path: &Path,
    config_path: &Path,
    backup_path: &Path,
    travel_passphrase: &[u8],
) -> Result<(), CryptoError> {
    // Read all source files (vault file is already encrypted, but we
    // re-encrypt the bundle under the travel passphrase)
    let vault_bytes = read_file_or_empty(vault_path)?;
    let db_bytes = read_file_or_empty(db_path)?;
    let config_bytes = read_file_or_empty(config_path)?;

    // Build payload: [len (8 LE)] [bytes] for each file
    let mut payload = Vec::new();
    write_length_prefixed(&mut payload, &vault_bytes);
    write_length_prefixed(&mut payload, &db_bytes);
    write_length_prefixed(&mut payload, &config_bytes);

    // Derive encryption key from travel passphrase
    let salt = kdf::generate_salt();
    let key = kdf::derive_key(travel_passphrase, &salt)?;

    // Encrypt payload
    let nonce = XChaCha20Poly1305::generate_nonce(&mut rand::rngs::OsRng);
    let cipher = crate::secret_use::with_key32(&key, |k| XChaCha20Poly1305::new_from_slice(k))
        .map_err(|_| CryptoError::KeyGeneration("invalid key length".into()))?;

    let ciphertext = cipher
        .encrypt(&nonce, payload.as_slice())
        .map_err(|_| CryptoError::KeyGeneration("backup encryption failed".into()))?;
    drop(key); // SecretBox zeroizes on drop

    // Write: [salt (16)] [nonce (24)] [ciphertext]
    let mut file_data = Vec::with_capacity(16 + 24 + ciphertext.len());
    file_data.extend_from_slice(&salt);
    file_data.extend_from_slice(nonce.as_slice());
    file_data.extend_from_slice(&ciphertext);

    std::fs::write(backup_path, &file_data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(backup_path, perms)?;
    }

    Ok(())
}

/// Restore files from an encrypted backup.
///
/// Decrypts the backup and writes vault, database, and config files
/// to their original locations.
pub fn import_backup(
    backup_path: &Path,
    vault_path: &Path,
    db_path: &Path,
    config_path: &Path,
    travel_passphrase: &[u8],
) -> Result<(), CryptoError> {
    let data = std::fs::read(backup_path)?;

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

    // Derive key from travel passphrase
    let key = kdf::derive_key(travel_passphrase, &salt)?;

    let nonce = chacha20poly1305::XNonce::from_slice(&nonce_bytes);
    let cipher = crate::secret_use::with_key32(&key, |k| XChaCha20Poly1305::new_from_slice(k))
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    drop(key); // SecretBox zeroizes on drop

    // Parse payload: [len (8 LE)] [bytes] for each file
    let mut cursor = 0;
    let vault_bytes = read_length_prefixed(&plaintext, &mut cursor)?;
    let db_bytes = read_length_prefixed(&plaintext, &mut cursor)?;
    let config_bytes = read_length_prefixed(&plaintext, &mut cursor)?;

    // Write restored files
    write_file_with_parents(vault_path, &vault_bytes)?;
    write_file_with_parents(db_path, &db_bytes)?;
    write_file_with_parents(config_path, &config_bytes)?;

    Ok(())
}

/// Read a file, returning empty Vec if it doesn't exist.
fn read_file_or_empty(path: &Path) -> Result<Vec<u8>, CryptoError> {
    if path.exists() {
        Ok(std::fs::read(path)?)
    } else {
        Ok(Vec::new())
    }
}

/// Write [length (8 LE)] [bytes] to a buffer.
fn write_length_prefixed(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u64).to_le_bytes());
    buf.extend_from_slice(data);
}

/// Read [length (8 LE)] [bytes] from a buffer at the given cursor position.
fn read_length_prefixed(data: &[u8], cursor: &mut usize) -> Result<Vec<u8>, CryptoError> {
    if *cursor + 8 > data.len() {
        return Err(CryptoError::VaultDecryptionFailed);
    }
    let len_bytes: [u8; 8] = data[*cursor..*cursor + 8]
        .try_into()
        .map_err(|_| CryptoError::VaultDecryptionFailed)?;
    let len = u64::from_le_bytes(len_bytes) as usize;
    *cursor += 8;

    if *cursor + len > data.len() {
        return Err(CryptoError::VaultDecryptionFailed);
    }
    let bytes = data[*cursor..*cursor + len].to_vec();
    *cursor += len;
    Ok(bytes)
}

/// Write a file, creating parent directories if needed.
fn write_file_with_parents(path: &Path, data: &[u8]) -> Result<(), CryptoError> {
    if data.is_empty() {
        return Ok(()); // Don't create empty files
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_and_restore_roundtrip() {
        let dir = tempfile::tempdir().unwrap();

        // Create source files
        let vault_path = dir.path().join("vault.bin");
        let db_path = dir.path().join("springtale.db");
        let config_path = dir.path().join("springtale.toml");
        let backup_path = dir.path().join("backup.enc");

        std::fs::write(&vault_path, b"VAULT_DATA_SECRET").unwrap();
        std::fs::write(&db_path, b"DATABASE_CONTENT").unwrap();
        std::fs::write(&config_path, b"[store]\npath = \"/data\"").unwrap();

        // Export backup
        let passphrase = b"travel-passphrase-2024";
        export_backup(
            &vault_path,
            &db_path,
            &config_path,
            &backup_path,
            passphrase,
        )
        .unwrap();
        assert!(backup_path.exists());

        // Wipe originals
        std::fs::remove_file(&vault_path).unwrap();
        std::fs::remove_file(&db_path).unwrap();
        std::fs::remove_file(&config_path).unwrap();
        assert!(!vault_path.exists());

        // Restore
        let restore_dir = dir.path().join("restored");
        let r_vault = restore_dir.join("vault.bin");
        let r_db = restore_dir.join("springtale.db");
        let r_config = restore_dir.join("springtale.toml");
        import_backup(&backup_path, &r_vault, &r_db, &r_config, passphrase).unwrap();

        assert_eq!(std::fs::read(&r_vault).unwrap(), b"VAULT_DATA_SECRET");
        assert_eq!(std::fs::read(&r_db).unwrap(), b"DATABASE_CONTENT");
        assert_eq!(
            std::fs::read_to_string(&r_config).unwrap(),
            "[store]\npath = \"/data\""
        );
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let dir = tempfile::tempdir().unwrap();

        let vault_path = dir.path().join("vault.bin");
        let db_path = dir.path().join("springtale.db");
        let config_path = dir.path().join("springtale.toml");
        let backup_path = dir.path().join("backup.enc");

        std::fs::write(&vault_path, b"secret").unwrap();
        std::fs::write(&db_path, b"data").unwrap();
        std::fs::write(&config_path, b"config").unwrap();

        export_backup(
            &vault_path,
            &db_path,
            &config_path,
            &backup_path,
            b"correct",
        )
        .unwrap();

        let result = import_backup(
            &backup_path,
            &dir.path().join("r_vault"),
            &dir.path().join("r_db"),
            &dir.path().join("r_config"),
            b"wrong",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_file_looks_random() {
        let dir = tempfile::tempdir().unwrap();

        let vault_path = dir.path().join("vault.bin");
        let db_path = dir.path().join("db");
        let config_path = dir.path().join("config");
        let backup_path = dir.path().join("backup.enc");

        std::fs::write(&vault_path, b"data").unwrap();
        std::fs::write(&db_path, b"").unwrap();
        std::fs::write(&config_path, b"").unwrap();

        export_backup(&vault_path, &db_path, &config_path, &backup_path, b"pass").unwrap();

        let backup_bytes = std::fs::read(&backup_path).unwrap();
        // Should not contain any recognizable strings
        let text = String::from_utf8_lossy(&backup_bytes);
        assert!(!text.contains("data"));
        assert!(!text.contains("vault"));
        assert!(!text.contains("springtale"));
    }

    #[test]
    fn test_backup_with_missing_files() {
        let dir = tempfile::tempdir().unwrap();

        // Only vault exists — db and config are missing
        let vault_path = dir.path().join("vault.bin");
        let db_path = dir.path().join("nonexistent.db");
        let config_path = dir.path().join("nonexistent.toml");
        let backup_path = dir.path().join("backup.enc");

        std::fs::write(&vault_path, b"vault_only").unwrap();

        export_backup(&vault_path, &db_path, &config_path, &backup_path, b"pass").unwrap();

        let r_vault = dir.path().join("r_vault");
        let r_db = dir.path().join("r_db");
        let r_config = dir.path().join("r_config");
        import_backup(&backup_path, &r_vault, &r_db, &r_config, b"pass").unwrap();

        assert_eq!(std::fs::read(&r_vault).unwrap(), b"vault_only");
        // Missing files should not be created
        assert!(!r_db.exists());
        assert!(!r_config.exists());
    }
}
