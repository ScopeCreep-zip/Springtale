#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;

use super::apply::{apply, is_legacy_database, SCHEMA_VERSION};
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
