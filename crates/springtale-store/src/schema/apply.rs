use rusqlite::Connection;

use crate::error::StoreError;

/// Declarative schema version. Bump only when the canonical DDL
/// changes shape (table added, column dropped, etc.). The value is
/// written to `PRAGMA user_version` after a successful apply, and
/// checked on every subsequent open.
///
/// Version history:
/// - 1: launch baseline — the full DDL in `DDL_IN_ORDER`.
///
/// From launch, every schema change is a new additive
/// `upgrade_vN_to_vN+1` step applied by [`apply`]; the canonical DDL
/// always reflects the *current* shape for fresh databases.
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
    ("dedupe", include_str!("sql/dedupe.sql")),
    ("executions", include_str!("sql/executions.sql")),
    (
        "mental_model_workspaces",
        include_str!("sql/mental_model_workspaces.sql"),
    ),
    ("ai_token_usage", include_str!("sql/ai_token_usage.sql")),
    ("approvals", include_str!("sql/approvals.sql")),
];

/// Apply the declarative schema to a connection.
///
/// This function never destroys or rewrites existing data. It only
/// creates objects in a database that has none.
///
/// - Already applied (`user_version == SCHEMA_VERSION`): no-op.
/// - Any other non-zero `user_version`: returns
///   `StoreError::SchemaVersion` and leaves the database untouched.
/// - `user_version == 0` but `sqlite_master` is non-empty: the file
///   was not created by this schema (unknown origin). Returns
///   `StoreError::SchemaVersion` and leaves the database untouched.
/// - Truly empty database: runs all DDL inside a single transaction,
///   then sets `PRAGMA user_version = SCHEMA_VERSION` as the last
///   statement before commit so a partial apply never gets versioned.
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

    let objects: i64 =
        conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))?;
    if objects != 0 {
        tracing::warn!(
            objects,
            "database has no schema version but is not empty — refusing to touch it"
        );
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
