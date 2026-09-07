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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use springtale_crypto::vault::Vault;
    use springtale_store::SafetyConfigRow;
    use springtale_store::backend::SqliteBackend;
    use tempfile::TempDir;

    use super::*;

    /// Production stores are always encrypted (plan 0.5), so file-backed
    /// tests open with a fixed key. Never used outside tests.
    const TEST_DB_KEY_HEX: &str =
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    /// The passphrase protecting the vault itself — distinct from the
    /// travel passphrase so a test can prove the backup preserves the
    /// vault's own encryption rather than re-keying it.
    const VAULT_PASSPHRASE: &[u8] = b"vault-passphrase";
    const TRAVEL_PASSPHRASE: &[u8] = b"travel-passphrase";

    /// Markers that must never survive `prepare` in cleartext anywhere
    /// under the data directory — including inside the backup file.
    const VAULT_MARKER: &str = "PLAINTEXT-MARKER-VAULT-SECRET";
    const CONFIG_MARKER: &str = "PLAINTEXT-MARKER-CONFIG-TOKEN";
    const DB_MARKER: &str = "PLAINTEXT-MARKER-DB-ROW";

    /// A temp data directory laid out the way `springtale_store::paths`
    /// lays out the real one, so `secure_wipe_sqlite`'s `-wal`/`-shm`
    /// derivation (which keys off the `.db` extension) behaves as in
    /// production.
    struct Fixture {
        dir: TempDir,
        vault_path: PathBuf,
        db_path: PathBuf,
        config_path: PathBuf,
        backup_path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            Self {
                vault_path: root.join("vault.bin"),
                db_path: root.join("springtale.db"),
                config_path: root.join("springtale.toml"),
                // The backup lands inside the scanned tree on purpose:
                // the leak scan then also covers the backup itself.
                backup_path: root.join("travel.backup"),
                dir,
            }
        }

        fn root(&self) -> &Path {
            self.dir.path()
        }

        /// Lay down a real vault, config file and encrypted SQLite
        /// database, then hand back a live store handle.
        ///
        /// The writes go through a first handle that is dropped before
        /// the returned one is opened: closing the last connection
        /// checkpoints WAL into the `.db` file, which is what `prepare`
        /// actually copies.
        async fn populate(&self) -> SqliteBackend {
            let mut vault = Vault::create(&self.vault_path, VAULT_PASSPHRASE).unwrap();
            vault
                .set("api_token", VAULT_MARKER.as_bytes().to_vec())
                .unwrap();
            vault.save().unwrap();

            std::fs::write(
                &self.config_path,
                format!("[bot]\ntoken = \"{CONFIG_MARKER}\"\n"),
            )
            .unwrap();

            {
                let writer = SqliteBackend::open_encrypted(&self.db_path, TEST_DB_KEY_HEX).unwrap();
                let config = SafetyConfigRow {
                    window_title: DB_MARKER.to_owned(),
                    ..Default::default()
                };
                writer.upsert_safety_config(&config).await.unwrap();
            }

            SqliteBackend::open_encrypted(&self.db_path, TEST_DB_KEY_HEX).unwrap()
        }

        fn prepare_with(&self, store: &dyn StorageBackend) -> Result<(), OperationError> {
            prepare(
                &self.vault_path,
                &self.db_path,
                &self.config_path,
                &self.backup_path,
                TRAVEL_PASSPHRASE,
                store,
            )
        }

        fn restore_with(&self, passphrase: &[u8]) -> Result<(), OperationError> {
            restore(
                &self.backup_path,
                &self.vault_path,
                &self.db_path,
                &self.config_path,
                passphrase,
            )
        }
    }

    /// Every regular file under `root`, paired with its bytes.
    fn read_tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    found.push((path, bytes));
                }
            }
        }
        found
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[tokio::test]
    async fn test_prepare_produces_backup_the_vault_can_reopen() {
        let fx = Fixture::new();
        let store = fx.populate().await;

        fx.prepare_with(&store).unwrap();
        drop(store);

        assert!(fx.backup_path.exists(), "prepare must leave a backup");
        assert!(
            !fx.vault_path.exists(),
            "prepare must wipe the vault it just backed up"
        );

        // The backup is now the only route back to the vault, and the
        // vault's own passphrase (not the travel one) must still open it.
        fx.restore_with(TRAVEL_PASSPHRASE).unwrap();

        let vault = Vault::open(&fx.vault_path, VAULT_PASSPHRASE).unwrap();
        assert_eq!(
            vault.get("api_token").unwrap().map(Vec::as_slice),
            Some(VAULT_MARKER.as_bytes()),
        );
    }

    #[tokio::test]
    async fn test_prepare_leaves_no_plaintext_on_disk() {
        let fx = Fixture::new();
        let store = fx.populate().await;

        fx.prepare_with(&store).unwrap();

        assert!(!fx.vault_path.exists(), "vault file survived prepare");
        assert!(!fx.db_path.exists(), "database file survived prepare");
        assert!(!fx.config_path.exists(), "config file survived prepare");
        assert!(
            !fx.db_path.with_extension("db-wal").exists(),
            "WAL journal survived prepare"
        );
        assert!(
            !fx.db_path.with_extension("db-shm").exists(),
            "shared-memory index survived prepare"
        );

        drop(store);

        // Nothing left anywhere under the data directory — the backup
        // included — may carry a marker in the clear.
        for (path, bytes) in read_tree(fx.root()) {
            for marker in [VAULT_MARKER, CONFIG_MARKER, DB_MARKER] {
                assert!(
                    !contains(&bytes, marker.as_bytes()),
                    "{marker} left in cleartext in {}",
                    path.display(),
                );
            }
        }
    }

    #[tokio::test]
    async fn test_restore_round_trips_vault_db_and_config() {
        let fx = Fixture::new();

        let (vault_before, db_before, config_before) = {
            let store = fx.populate().await;
            let snapshot = (
                std::fs::read(&fx.vault_path).unwrap(),
                std::fs::read(&fx.db_path).unwrap(),
                std::fs::read(&fx.config_path).unwrap(),
            );
            fx.prepare_with(&store).unwrap();
            snapshot
        };

        fx.restore_with(TRAVEL_PASSPHRASE).unwrap();

        assert_eq!(std::fs::read(&fx.vault_path).unwrap(), vault_before);
        assert_eq!(std::fs::read(&fx.db_path).unwrap(), db_before);
        assert_eq!(std::fs::read(&fx.config_path).unwrap(), config_before);

        // Byte identity is necessary but not sufficient: the restored
        // database must still decrypt and serve the row written before
        // departure.
        let store = SqliteBackend::open_encrypted(&fx.db_path, TEST_DB_KEY_HEX).unwrap();
        let restored = store.get_safety_config().await.unwrap();
        assert_eq!(
            restored.map(|c| c.window_title).as_deref(),
            Some(DB_MARKER),
            "restored database lost the row written before travel"
        );
    }

    #[test]
    fn test_restore_missing_backup_returns_not_found() {
        let fx = Fixture::new();

        let err = fx.restore_with(TRAVEL_PASSPHRASE).unwrap_err();

        assert!(matches!(err, OperationError::NotFound(_)), "got {err:?}");
        assert!(!fx.vault_path.exists());
        assert!(!fx.config_path.exists());
    }

    #[tokio::test]
    async fn test_restore_with_wrong_passphrase_writes_nothing() {
        let fx = Fixture::new();
        let store = fx.populate().await;
        fx.prepare_with(&store).unwrap();
        drop(store);

        let err = fx.restore_with(b"not-the-travel-passphrase").unwrap_err();

        assert!(matches!(err, OperationError::Rule(_)), "got {err:?}");
        assert!(
            !fx.vault_path.exists(),
            "a failed restore must not resurrect the vault"
        );
        assert!(
            !fx.config_path.exists(),
            "a failed restore must not resurrect the config"
        );
        assert!(
            !fx.db_path.exists(),
            "a failed restore must not resurrect the database"
        );
    }
}
