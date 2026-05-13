use rusqlite::{Connection, OptionalExtension};

use crate::error::StoreError;

/// Declarative schema version. Bump only when the canonical DDL
/// changes shape (table added, column dropped, etc.). The value is
/// written to `PRAGMA user_version` after a successful apply, and
/// checked on every subsequent open.
///
/// Version history:
/// - 1: initial schema
/// - 2: Phase 0.4 — `rules.owner_kind` / `owner_agent_id` /
///   `owner_formation_id` columns + indexes for cooperation scoping
/// - 3: Phase A — `dedupe_seen` table for `Action::Dedupe` state
/// - 4: Phase B — `executions` + `execution_steps` tables for the
///   privacy-default observability log (sizes-only, 14d retention)
/// - 5: D1 — `mental_model_workspaces` table for the external-
///   destination directory (Telegram chats / Discord channels /
///   Signal groups / IRC channels / Nostr pubkeys / Bluesky
///   accounts), extending the SharedMentalModel
pub const SCHEMA_VERSION: i32 = 5;

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
    ("mental_model_workspaces", include_str!("sql/mental_model_workspaces.sql")),
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

    // In-place upgrade path. Each step bumps one version and is
    // additive only (ALTER TABLE ADD COLUMN, CREATE INDEX, CREATE
    // TABLE for new domains) so dev databases survive Phase 0+
    // schema bumps without a panic-wipe. Per the declarative-schema
    // contract, the canonical DDL in `DDL_IN_ORDER` always reflects
    // the *current* shape — these in-place steps make older
    // databases match.
    if found == 1 && SCHEMA_VERSION >= 2 {
        upgrade_v1_to_v2(conn)?;
        // Continue to v2 → v3 below.
    }
    let interim: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if interim == 2 && SCHEMA_VERSION >= 3 {
        upgrade_v2_to_v3(conn)?;
    }
    let interim: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if interim == 3 && SCHEMA_VERSION >= 4 {
        upgrade_v3_to_v4(conn)?;
    }
    let interim: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if interim == 4 && SCHEMA_VERSION >= 5 {
        upgrade_v4_to_v5(conn)?;
    }

    let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current == SCHEMA_VERSION {
        tracing::info!(version = SCHEMA_VERSION, "declarative schema upgraded in place");
        return Ok(());
    }
    if current != 0 {
        return Err(StoreError::SchemaVersion {
            found: current,
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

/// D1 — add `mental_model_workspaces` table for the external-
/// destination directory. Additive only; no existing rows
/// touched.
fn upgrade_v4_to_v5(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("sql/mental_model_workspaces.sql"))
        .map_err(|e| StoreError::Schema(format!("upgrade v4→v5: {e}")))?;
    tx.execute_batch("PRAGMA user_version = 5;")?;
    tx.commit()?;
    tracing::info!("schema upgraded v4 → v5 (mental_model_workspaces table)");
    Ok(())
}

/// Phase B — add `executions` + `execution_steps` tables for the
/// privacy-default executions log. Additive only.
fn upgrade_v3_to_v4(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("sql/executions.sql"))
        .map_err(|e| StoreError::Schema(format!("upgrade v3→v4: {e}")))?;
    tx.execute_batch("PRAGMA user_version = 4;")?;
    tx.commit()?;
    tracing::info!("schema upgraded v3 → v4 (executions + execution_steps tables)");
    Ok(())
}

/// Phase A — add `dedupe_seen` table for `Action::Dedupe` state.
/// Additive only; no existing rows touched.
fn upgrade_v2_to_v3(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("sql/dedupe.sql"))
        .map_err(|e| StoreError::Schema(format!("upgrade v2→v3: {e}")))?;
    tx.execute_batch("PRAGMA user_version = 3;")?;
    tx.commit()?;
    tracing::info!("schema upgraded v2 → v3 (dedupe_seen table)");
    Ok(())
}

/// Phase 0.4 — add `RuleOwner` columns to the `rules` table. Existing
/// rows backfill to `owner_kind = 'global'` (the column default), so
/// every pre-Phase-0 rule continues to fire from any context.
fn upgrade_v1_to_v2(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        r#"
        ALTER TABLE rules ADD COLUMN owner_kind         TEXT NOT NULL DEFAULT 'global';
        ALTER TABLE rules ADD COLUMN owner_agent_id     TEXT;
        ALTER TABLE rules ADD COLUMN owner_formation_id TEXT;
        CREATE INDEX IF NOT EXISTS idx_rules_owner_kind      ON rules(owner_kind);
        CREATE INDEX IF NOT EXISTS idx_rules_owner_agent     ON rules(owner_agent_id);
        CREATE INDEX IF NOT EXISTS idx_rules_owner_formation ON rules(owner_formation_id);
        PRAGMA user_version = 2;
        "#,
    )
    .map_err(|e| StoreError::Schema(format!("upgrade v1→v2: {e}")))?;
    tx.commit()?;
    tracing::info!("schema upgraded v1 → v2 (rule owner columns)");
    Ok(())
}
