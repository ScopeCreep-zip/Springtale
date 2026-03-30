use rusqlite::Connection;

use crate::error::StoreError;

/// Embedded SQL for schema version 1.
const MIGRATION_001: &str = include_str!("001_init.sql");

/// All migrations in order.
const MIGRATIONS: &[(i64, &str)] = &[(1, MIGRATION_001)];

/// Run all pending migrations on the given connection.
///
/// Migrations are idempotent (CREATE IF NOT EXISTS). The `_migrations`
/// table tracks which versions have been applied.
pub fn run_migrations(conn: &Connection) -> Result<(), StoreError> {
    // Ensure the migrations table exists
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(|e| StoreError::Migration(format!("create _migrations table: {e}")))?;

    for (version, sql) in MIGRATIONS {
        let already_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE version = ?1",
                rusqlite::params![version],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Migration(format!("check migration {version}: {e}")))?;

        if already_applied {
            tracing::debug!(version = version, "migration already applied, skipping");
            continue;
        }

        tracing::info!(version = version, "applying migration");

        conn.execute_batch(sql)
            .map_err(|e| StoreError::Migration(format!("apply migration {version}: {e}")))?;

        conn.execute(
            "INSERT INTO _migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| StoreError::Migration(format!("record migration {version}: {e}")))?;

        tracing::info!(version = version, "migration applied");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_migrations_on_new_db() {
        let conn = Connection::open_in_memory().unwrap();
        let result = run_migrations(&conn);
        assert!(result.is_ok(), "migration failed: {:?}", result.err());

        // Verify tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='rules'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_run_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        // Running again should not fail
        let result = run_migrations(&conn);
        assert!(result.is_ok());
    }

    #[test]
    fn test_migration_version_recorded() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }
}
