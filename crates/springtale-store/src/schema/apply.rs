use rusqlite::{Connection, OptionalExtension};

use crate::error::StoreError;

/// Declarative schema version. Bump only when the canonical DDL
/// changes shape (table added, column dropped, etc.). The value is
/// written to `PRAGMA user_version` after a successful apply, and
/// checked on every subsequent open.
pub const SCHEMA_VERSION: i32 = 1;

/// DDL applied in dependency order. Tables that reference other
/// tables (formation_members → formations) must come after their
/// targets so the foreign-key constraint resolves.
const DDL_IN_ORDER: &[(&str, &str)] = &[
    ("connectors", include_str!("sql/connectors.sql")),
    ("rules", include_str!("sql/rules.sql")),
    ("events", include_str!("sql/events.sql")),
    ("jobs", include_str!("sql/jobs.sql")),
    ("bot", include_str!("sql/bot.sql")),
    ("audit", include_str!("sql/audit.sql")),
    ("safety", include_str!("sql/safety.sql")),
    ("formations", include_str!("sql/formations.sql")),
    ("runtime_config", include_str!("sql/runtime_config.sql")),
    ("execution", include_str!("sql/execution.sql")),
    ("wasm", include_str!("sql/wasm.sql")),
    ("cooperation", include_str!("sql/cooperation.sql")),
];

/// Detect a database created under the (removed) numbered-migration
/// runner. The legacy runner kept a `_migrations` tracking table; its
/// presence is the unambiguous marker. In-memory and freshly-created
/// file DBs return `false`.
pub fn is_legacy_database(conn: &Connection) -> Result<bool, StoreError> {
    let found: Option<bool> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='_migrations'",
            [],
            |_| Ok(true),
        )
        .optional()?;
    Ok(found.unwrap_or(false))
}

/// Apply the declarative schema to a (presumed-fresh) connection.
///
/// - Fresh DB (`user_version = 0`): runs all DDL inside a single
///   transaction, then sets `PRAGMA user_version = SCHEMA_VERSION` as
///   the last statement before commit so a partial apply never gets
///   versioned.
/// - Already applied (`user_version == SCHEMA_VERSION`): no-op.
/// - Mismatch (non-zero, non-matching): returns
///   `StoreError::SchemaVersion`. Pre-launch, the SqliteBackend open
///   path auto-wipes legacy DBs, so this branch is only hit when a
///   newer-on-disk schema meets older code.
pub fn apply(conn: &Connection) -> Result<(), StoreError> {
    let found: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if found == SCHEMA_VERSION {
        tracing::debug!(version = SCHEMA_VERSION, "schema already current");
        return Ok(());
    }
    if found != 0 {
        return Err(StoreError::SchemaVersion {
            found,
            expected: SCHEMA_VERSION,
        });
    }

    let tx = conn.unchecked_transaction()?;
    for (domain, ddl) in DDL_IN_ORDER {
        tx.execute_batch(ddl)
            .map_err(|e| StoreError::Schema(format!("apply {domain}: {e}")))?;
    }
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
    tx.commit()?;
    tracing::info!(version = SCHEMA_VERSION, "declarative schema applied");
    Ok(())
}
