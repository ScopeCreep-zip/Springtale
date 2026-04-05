//! Travel mode operations — encrypted backup and restore.
//!
//! Per ARCHITECTURE.md §2.6:
//! - Pre-departure: export encrypted backup, wipe local data
//! - On arrival: restore from backup via QR code or encrypted file
//!
//! Passphrase handling is app-specific (CLI prompts, desktop uses IPC,
//! mobile uses biometric). These functions take the passphrase as bytes.

use std::path::Path;

use springtale_store::StorageBackend;

use crate::error::OperationError;

/// Create an encrypted backup then wipe all local data.
///
/// 1. Exports encrypted backup (vault + database + config)
/// 2. Wipes vault, database, and config files
///
/// The backup file is indistinguishable from random data without
/// the travel passphrase.
pub fn prepare(
    vault_path: &Path,
    db_path: &Path,
    config_path: &Path,
    backup_path: &Path,
    passphrase: &[u8],
    store: &dyn StorageBackend,
) -> Result<(), OperationError> {
    // Export encrypted backup
    springtale_crypto::vault::backup::export_backup(
        vault_path,
        db_path,
        config_path,
        backup_path,
        passphrase,
    )
    .map_err(|e| OperationError::Rule(format!("backup failed: {e}")))?;

    // Wipe vault file
    if vault_path.exists() {
        springtale_crypto::vault::wipe::wipe_vault_file(vault_path)
            .map_err(|e| OperationError::Rule(format!("vault wipe failed: {e}")))?;
    }

    // Wipe database
    store.panic_wipe().map_err(OperationError::Store)?;

    // Wipe config file
    if config_path.exists() {
        let _ = springtale_crypto::vault::wipe::wipe_vault_file(config_path);
    }

    Ok(())
}

/// Restore from an encrypted travel backup.
///
/// Decrypts and restores vault, database, and config files.
pub fn restore(
    backup_path: &Path,
    vault_path: &Path,
    db_path: &Path,
    config_path: &Path,
    passphrase: &[u8],
) -> Result<(), OperationError> {
    if !backup_path.exists() {
        return Err(OperationError::NotFound(format!(
            "backup file not found: {}",
            backup_path.display()
        )));
    }

    springtale_crypto::vault::backup::import_backup(
        backup_path,
        vault_path,
        db_path,
        config_path,
        passphrase,
    )
    .map_err(|e| OperationError::Rule(format!("restore failed: {e}")))?;

    Ok(())
}
