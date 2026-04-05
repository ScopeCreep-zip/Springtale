use std::fs::File;
use std::io::Write;
use std::path::Path;

use rand::RngCore;

use crate::error::StoreError;

/// Securely overwrite a file with random bytes, then delete it.
///
/// Single-pass random overwrite per NIST 800-88. On SSDs with wear
/// leveling, this cannot guarantee physical block erasure — the
/// database's encryption is the primary defense.
///
/// Handles the main .db file plus SQLite WAL mode artifacts
/// (.db-wal, .db-shm) which may contain recoverable data.
pub fn secure_wipe_file(path: &Path) -> Result<(), StoreError> {
    if !path.exists() {
        return Ok(());
    }

    let file_size = std::fs::metadata(path)?.len() as usize;

    let mut file = File::options().write(true).open(path)?;
    let mut rng = rand::thread_rng();
    let mut buf = [0u8; 4096];
    let mut remaining = file_size;

    while remaining > 0 {
        rng.fill_bytes(&mut buf);
        let to_write = remaining.min(buf.len());
        file.write_all(&buf[..to_write])?;
        remaining -= to_write;
    }

    file.sync_all()?;
    drop(file);

    std::fs::remove_file(path)?;
    Ok(())
}

/// Wipe all SQLite-related files for a database path.
///
/// SQLite in WAL mode creates .db-wal and .db-shm alongside the
/// main .db file. All three must be wiped to prevent data recovery.
pub fn secure_wipe_sqlite(db_path: &Path) -> Result<(), StoreError> {
    // Main database file
    secure_wipe_file(db_path)?;

    // WAL journal (contains uncommitted writes)
    let wal_path = db_path.with_extension("db-wal");
    secure_wipe_file(&wal_path)?;

    // Shared memory index (contains page references)
    let shm_path = db_path.with_extension("db-shm");
    secure_wipe_file(&shm_path)?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_wipe_file_destroys_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        std::fs::write(&path, b"SENSITIVE_DATABASE_DATA").unwrap();
        assert!(path.exists());

        secure_wipe_file(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_secure_wipe_sqlite_removes_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let wal_path = dir.path().join("test.db-wal");
        let shm_path = dir.path().join("test.db-shm");

        std::fs::write(&db_path, b"main db").unwrap();
        std::fs::write(&wal_path, b"wal data").unwrap();
        std::fs::write(&shm_path, b"shm data").unwrap();

        secure_wipe_sqlite(&db_path).unwrap();

        assert!(!db_path.exists());
        assert!(!wal_path.exists());
        assert!(!shm_path.exists());
    }

    #[test]
    fn test_secure_wipe_nonexistent_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.db");
        assert!(secure_wipe_file(&path).is_ok());
    }

    #[test]
    fn test_secure_wipe_sqlite_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Only main file exists (no WAL/SHM)
        std::fs::write(&db_path, b"main db").unwrap();

        secure_wipe_sqlite(&db_path).unwrap();
        assert!(!db_path.exists());
    }
}
