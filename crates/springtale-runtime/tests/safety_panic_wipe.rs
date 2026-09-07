//! `operations::safety::panic_wipe` against a real data directory.
//!
//! `panic_wipe` resolves the vault and config paths itself, via
//! `springtale_store::paths`, which reads `XDG_DATA_HOME`. Redirecting
//! that variable is the only way to exercise the function without
//! destroying the developer's actual vault — so this file holds exactly
//! one test and owns its process's environment.

#![allow(clippy::unwrap_used)]

use springtale_store::SafetyConfigRow;
use springtale_store::StorageBackend;
use springtale_store::backend::SqliteBackend;
use tempfile::tempdir;

/// Production stores are always encrypted (plan 0.5), so file-backed
/// tests open with a fixed key. Never used outside tests.
const TEST_KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

const VAULT_MARKER: &str = "PLAINTEXT-MARKER-VAULT-SECRET";
const CONFIG_MARKER: &str = "PLAINTEXT-MARKER-CONFIG-TOKEN";
const DB_MARKER: &str = "PLAINTEXT-MARKER-DB-ROW";

/// Emergency wipe must leave the vault, the SQLite database, both WAL
/// artifacts and the config file gone — and no marker readable anywhere
/// under the data directory.
#[test]
fn test_panic_wipe_destroys_vault_db_wal_shm_and_config() {
    let dir = tempdir().unwrap();

    // SAFETY: `set_var` is only unsound while another thread may read
    // the environment concurrently. This is the sole test in this
    // binary and runs before any runtime, task or blocking pool exists,
    // so no other thread is alive to observe the write.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", dir.path());
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let vault_path = springtale_store::paths::default_vault_path();
        let db_path = springtale_store::paths::default_db_path();
        let config_path = springtale_store::paths::default_config_path();
        let data_dir = springtale_store::paths::data_dir();

        // Guard against a paths change silently pointing this test at
        // the real home directory.
        assert!(
            data_dir.starts_with(dir.path()),
            "data dir {} escaped the temp root",
            data_dir.display()
        );
        std::fs::create_dir_all(&data_dir).unwrap();

        let mut vault =
            springtale_crypto::vault::Vault::create(&vault_path, b"vault-pass").unwrap();
        vault
            .set("api_token", VAULT_MARKER.as_bytes().to_vec())
            .unwrap();
        vault.save().unwrap();

        std::fs::write(
            &config_path,
            format!("[bot]\ntoken = \"{CONFIG_MARKER}\"\n"),
        )
        .unwrap();

        let store = SqliteBackend::open_encrypted(&db_path, TEST_KEY_HEX).unwrap();
        let config = SafetyConfigRow {
            window_title: DB_MARKER.to_owned(),
            ..Default::default()
        };
        store.upsert_safety_config(&config).await.unwrap();

        // WAL mode: the write must have produced both journal artifacts,
        // otherwise the wipe below would be proving nothing about them.
        let wal_path = db_path.with_extension("db-wal");
        let shm_path = db_path.with_extension("db-shm");
        assert!(vault_path.exists());
        assert!(db_path.exists());
        assert!(wal_path.exists(), "expected a WAL journal to wipe");
        assert!(shm_path.exists(), "expected a shared-memory index to wipe");
        assert!(config_path.exists());

        springtale_runtime::operations::safety::panic_wipe(&store)
            .await
            .unwrap();

        assert!(!vault_path.exists(), "vault survived panic wipe");
        assert!(!db_path.exists(), "database survived panic wipe");
        assert!(!wal_path.exists(), "WAL journal survived panic wipe");
        assert!(
            !shm_path.exists(),
            "shared-memory index survived panic wipe"
        );
        assert!(!config_path.exists(), "config survived panic wipe");

        drop(store);

        for (path, bytes) in read_tree(dir.path()) {
            for marker in [VAULT_MARKER, CONFIG_MARKER, DB_MARKER] {
                assert!(
                    !contains(&bytes, marker.as_bytes()),
                    "{marker} readable after panic wipe in {}",
                    path.display()
                );
            }
        }
    });
}

/// Every regular file under `root`, paired with its bytes.
fn read_tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
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
