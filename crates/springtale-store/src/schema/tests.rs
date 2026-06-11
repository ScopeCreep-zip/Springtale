#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;

use super::apply::{SCHEMA_VERSION, apply, is_legacy_database};
use crate::error::StoreError;

fn fresh_conn() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    c
}

#[test]
fn apply_on_fresh_db_creates_tables_and_sets_user_version() {
    let c = fresh_conn();
    apply(&c).unwrap();

    let v: i32 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);

    for table in &[
        "rules",
        "connectors",
        "events",
        "jobs",
        "bot_sessions",
        "user_prefs",
        "bot_memory",
        "bot_aliases",
        "audit_trail",
        "safety_config",
        "formations",
        "formation_members",
        "formation_momentum",
        "formation_rally",
        "config_store",
        "execution_results",
        "wasm_binaries",
        "coop_writes",
        "coop_deposits",
        "mental_model_domain",
        "mental_model_capability",
        "mental_model_pattern",
        "mental_model_vocabulary",
        "mental_model_convention",
    ] {
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "table {table} missing after apply");
    }
}

#[test]
fn apply_is_idempotent() {
    let c = fresh_conn();
    apply(&c).unwrap();
    // second call hits the no-op branch (user_version already set)
    apply(&c).unwrap();
    let v: i32 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);
}

#[test]
fn apply_rejects_mismatched_user_version() {
    let c = fresh_conn();
    c.execute_batch("PRAGMA user_version = 99;").unwrap();
    let err = apply(&c).unwrap_err();
    match err {
        StoreError::SchemaVersion { found, expected } => {
            assert_eq!(found, 99);
            assert_eq!(expected, SCHEMA_VERSION);
        }
        other => panic!("expected SchemaVersion, got {other:?}"),
    }
}

#[test]
fn is_legacy_database_detects_old_marker_table() {
    let c = fresh_conn();
    assert!(!is_legacy_database(&c).unwrap());
    c.execute_batch(
        "CREATE TABLE _migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
    )
    .unwrap();
    assert!(is_legacy_database(&c).unwrap());
}

#[test]
fn apply_upgrades_v1_database_in_place_to_current_schema() {
    // Simulate a dev database left at the v1 schema: create the v1
    // `rules` table shape (no owner columns), bump user_version to 1,
    // then call apply() and verify the upgrade-in-place path adds the
    // owner columns without dropping existing rows.
    let c = fresh_conn();
    c.execute_batch(
        r#"
        CREATE TABLE rules (
            id               TEXT    PRIMARY KEY,
            name             TEXT    NOT NULL,
            description      TEXT    NOT NULL DEFAULT '',
            status           TEXT    NOT NULL DEFAULT 'enabled',
            version          INTEGER NOT NULL DEFAULT 1,
            trigger_type     TEXT    NOT NULL,
            rule_toml        TEXT    NOT NULL,
            activation_error TEXT,
            created_at       TEXT    NOT NULL,
            updated_at       TEXT    NOT NULL
        );
        INSERT INTO rules (id, name, trigger_type, rule_toml, created_at, updated_at)
        VALUES ('legacy-1', 'pre-phase-0', 'Cron', 'name = "pre"', '2026-01-01', '2026-01-01');
        PRAGMA user_version = 1;
        "#,
    )
    .unwrap();

    apply(&c).unwrap();

    // user_version bumped to current.
    let v: i32 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION);

    // Owner columns exist (column-exists check via PRAGMA).
    let cols: Vec<String> = c
        .prepare("PRAGMA table_info(rules)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap();
    assert!(
        cols.contains(&"owner_kind".to_owned()),
        "owner_kind column not added"
    );
    assert!(
        cols.contains(&"owner_agent_id".to_owned()),
        "owner_agent_id column not added"
    );
    assert!(
        cols.contains(&"owner_formation_id".to_owned()),
        "owner_formation_id column not added"
    );

    // Existing row survives and backfills to owner_kind = 'global'.
    let (id, kind): (String, String) = c
        .query_row(
            "SELECT id, owner_kind FROM rules WHERE id = 'legacy-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(id, "legacy-1");
    assert_eq!(kind, "global", "pre-Phase-0 row should backfill to global");
}

#[test]
fn seed_rows_present_after_apply() {
    let c = fresh_conn();
    apply(&c).unwrap();
    let count: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM config_store \
             WHERE key IN ('ai_adapter','safety','heartbeat_interval_secs')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 3, "runtime_config seed rows missing");
}
