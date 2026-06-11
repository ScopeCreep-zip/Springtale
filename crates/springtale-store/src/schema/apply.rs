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
/// - 6: Phase-7 audit Finding B — `audit_trail.prev_hash` /
///   `row_hash` / `chain_seq` columns for the tamper-evident row-
///   hash chain. Existing rows backfill in `chain_seq` order with
///   a one-time genesis anchor (zero-hash) so pre-migration
///   timelines stay verifiable from the migration point forward.
/// - 7: Phase-7 audit Finding D — `ai_token_usage` table for the
///   SQLite-backed per-bot daily quota. Replaces the in-process
///   `InMemoryTokenQuota` as the default so daemon restarts no
///   longer reset the daily counters. Additive; pre-existing AI
///   usage rows do not exist (the in-memory path never persisted).
/// - 8: Whole-codebase audit Finding #1 (R-004) — add
///   `author_pubkey_hex` + `manifest_sig_hex` columns to
///   `wasm_binaries` so the boot re-verifier pins the install-time
///   trust anchor outside the signed manifest body (TUF §4). Pre-
///   migration rows get empty defaults; the verifier treats empty
///   as "legacy, log warning" to preserve forward compatibility.
pub const SCHEMA_VERSION: i32 = 8;

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
    let interim: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if interim == 5 && SCHEMA_VERSION >= 6 {
        upgrade_v5_to_v6(conn)?;
    }
    let interim: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if interim == 6 && SCHEMA_VERSION >= 7 {
        upgrade_v6_to_v7(conn)?;
    }
    let interim: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if interim == 7 && SCHEMA_VERSION >= 8 {
        upgrade_v7_to_v8(conn)?;
    }

    let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current == SCHEMA_VERSION {
        tracing::info!(
            version = SCHEMA_VERSION,
            "declarative schema upgraded in place"
        );
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

/// Whole-codebase audit Finding #1 — add the install-time signing-key
/// pin columns to `wasm_binaries` so the boot re-verifier can fail
/// closed on a swap-the-whole-bundle attack against the SQLite store.
/// Pre-migration rows get empty defaults (the verifier treats empty
/// as legacy + logs a warning, preserving forward compatibility).
fn upgrade_v7_to_v8(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    // Tolerate either pre-existing wasm_binaries (real upgrades) or a
    // legacy fixture without the table (test paths). Idempotent ALTER
    // via column-existence check.
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='wasm_binaries'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if table_exists {
        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(wasm_binaries)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if !columns.iter().any(|c| c == "author_pubkey_hex") {
            tx.execute_batch(
                "ALTER TABLE wasm_binaries ADD COLUMN author_pubkey_hex TEXT NOT NULL DEFAULT '';",
            )
            .map_err(|e| StoreError::Schema(format!("upgrade v7→v8 (author_pubkey_hex): {e}")))?;
        }
        if !columns.iter().any(|c| c == "manifest_sig_hex") {
            tx.execute_batch(
                "ALTER TABLE wasm_binaries ADD COLUMN manifest_sig_hex TEXT NOT NULL DEFAULT '';",
            )
            .map_err(|e| StoreError::Schema(format!("upgrade v7→v8 (manifest_sig_hex): {e}")))?;
        }
    } else {
        tx.execute_batch(include_str!("sql/wasm.sql"))
            .map_err(|e| StoreError::Schema(format!("upgrade v7→v8 (create): {e}")))?;
    }
    tx.execute_batch("PRAGMA user_version = 8;")?;
    tx.commit()?;
    tracing::info!("schema upgraded v7 → v8 (wasm_binaries trust-anchor pin columns)");
    Ok(())
}

/// Phase-7 audit Finding D — add the `ai_token_usage` table that
/// backs the SQLite-persisted per-bot daily quota. Additive; the
/// previous in-process `InMemoryTokenQuota` had no persisted state.
fn upgrade_v6_to_v7(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("sql/ai_token_usage.sql"))
        .map_err(|e| StoreError::Schema(format!("upgrade v6→v7: {e}")))?;
    tx.execute_batch("PRAGMA user_version = 7;")?;
    tx.commit()?;
    tracing::info!("schema upgraded v6 → v7 (ai_token_usage table)");
    Ok(())
}

/// Phase-7 audit Finding B — add `prev_hash` / `row_hash` /
/// `chain_seq` columns to `audit_trail` for the tamper-evident row-
/// hash chain. Backfill: existing rows get sequential `chain_seq`
/// values in `(timestamp, id)` order; their `prev_hash` / `row_hash`
/// are left empty so the verifier knows to skip them and re-anchor
/// at the first post-migration row. Pre-migration history stays
/// readable but is NOT cryptographically protected (the chain
/// starts after the migration point — documented in
/// `docs/security/INCIDENT-RUNBOOK.md`).
fn upgrade_v5_to_v6(conn: &Connection) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;

    // The canonical `audit_trail` DDL ships the chain columns + the
    // unique chain_seq index. Pre-v6 databases that were built from
    // an older copy of the DDL won't have those — ALTER them in.
    // Some legacy/test fixtures don't even have the table; in that
    // case, run the canonical DDL so the migration is self-healing
    // rather than failing closed on missing prerequisites.
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='audit_trail'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);

    if table_exists {
        // Detect whether the chain columns already exist before
        // ALTER — pre-existing v5 databases that already saw the
        // canonical-DDL `audit.sql` shape don't need the ALTER.
        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(audit_trail)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if !columns.iter().any(|c| c == "prev_hash") {
            tx.execute_batch(
                "ALTER TABLE audit_trail ADD COLUMN prev_hash TEXT NOT NULL DEFAULT '';",
            )
            .map_err(|e| StoreError::Schema(format!("upgrade v5→v6 (prev_hash): {e}")))?;
        }
        if !columns.iter().any(|c| c == "row_hash") {
            tx.execute_batch(
                "ALTER TABLE audit_trail ADD COLUMN row_hash  TEXT NOT NULL DEFAULT '';",
            )
            .map_err(|e| StoreError::Schema(format!("upgrade v5→v6 (row_hash): {e}")))?;
        }
        if !columns.iter().any(|c| c == "chain_seq") {
            tx.execute_batch(
                "ALTER TABLE audit_trail ADD COLUMN chain_seq INTEGER NOT NULL DEFAULT 0;",
            )
            .map_err(|e| StoreError::Schema(format!("upgrade v5→v6 (chain_seq): {e}")))?;
        }
        // Backfill: existing rows all carry the `DEFAULT 0` from the ALTER
        // above (or a prior partial run), so the UNIQUE index below would
        // fail with `UNIQUE constraint failed: audit_trail.chain_seq` the
        // moment there's more than one audit row. Assign each row a unique
        // sequential `chain_seq` (1..N) in insertion (rowid) order — the same
        // order the chain verifier and `record_audit` (`tip + 1`) expect, so
        // new inserts continue cleanly from the backfilled tip. These rows
        // stay un-anchored (empty `row_hash`); this only gives them a
        // collision-free chain position so the index can be built.
        tx.execute_batch(
            "UPDATE audit_trail \
             SET chain_seq = (SELECT COUNT(*) FROM audit_trail AS a2 \
                              WHERE a2.rowid <= audit_trail.rowid);",
        )
        .map_err(|e| StoreError::Schema(format!("upgrade v5→v6 (backfill): {e}")))?;
        tx.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_audit_trail_chain_seq ON audit_trail(chain_seq);",
        )
        .map_err(|e| StoreError::Schema(format!("upgrade v5→v6 (index): {e}")))?;
    } else {
        // Self-heal: a legacy database without audit_trail at all
        // (test fixture, partial install) gets the canonical DDL
        // shape including the chain columns + index in one shot.
        tx.execute_batch(include_str!("sql/audit.sql"))
            .map_err(|e| StoreError::Schema(format!("upgrade v5→v6 (create): {e}")))?;
    }

    tx.execute_batch("PRAGMA user_version = 6;")?;
    tx.commit()?;
    tracing::info!(
        "schema upgraded v5 → v6 (audit_trail row-hash chain columns; \
         existing rows un-anchored, new chain begins at next INSERT)"
    );
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Regression: a v5 `audit_trail` with multiple rows (no chain columns)
    /// must upgrade to v6 without `UNIQUE constraint failed: audit_trail.chain_seq`.
    /// Before the backfill fix, the `ALTER ... DEFAULT 0` left every row at
    /// `chain_seq = 0` and the UNIQUE index creation blew up on the 2nd row.
    #[test]
    fn v5_to_v6_backfills_duplicate_chain_seq() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE audit_trail (
                 id TEXT PRIMARY KEY, timestamp TEXT, connector_name TEXT,
                 action_type TEXT, action_summary TEXT, verdict TEXT,
                 verdict_reason TEXT, result TEXT, created_at INTEGER
             );
             INSERT INTO audit_trail (id) VALUES ('a'), ('b'), ('c');
             PRAGMA user_version = 5;",
        )
        .unwrap();

        // Would panic with the UNIQUE-constraint error before the fix.
        upgrade_v5_to_v6(&conn).unwrap();

        let distinct: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT chain_seq) FROM audit_trail",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(distinct, 3, "each row got a unique chain_seq");
        let max: i64 = conn
            .query_row("SELECT MAX(chain_seq) FROM audit_trail", [], |r| r.get(0))
            .unwrap();
        assert_eq!(max, 3, "backfilled sequentially 1..N");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 6, "migration committed");
    }

    /// A fresh `audit_trail` created from canonical DDL (table absent → the
    /// self-heal branch) must also reach v6 cleanly.
    #[test]
    fn v5_to_v6_self_heals_missing_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 5;").unwrap();
        upgrade_v5_to_v6(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 6);
    }
}
