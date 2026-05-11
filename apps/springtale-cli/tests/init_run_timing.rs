//! J6 — `springtale init && springtale run` ≤60s wall-clock test.
//!
//! Per `COOPERATION_IMPLEMENTATION_PLAN.md §16 criterion 4`: fresh
//! install to first running daemon must complete in 60 seconds or
//! less. This test exercises the programmatic equivalents of the
//! `init` and `run` paths under a fresh temp directory and asserts
//! the wall-clock budget.
//!
//! ## What's measured
//!
//! - **`init` phase:**
//!   - Vault creation with a known passphrase (Argon2id KDF — the
//!     largest single cost on this path).
//!   - SQLite store opening + schema apply.
//!   - Bootstrap config (data dir scaffolding).
//! - **`run` phase:**
//!   - `springtale_runtime::init::init(...)` — registry, AI adapter,
//!     WASM engine, capability bridge, role registry, all sub-buses.
//!
//! The two phases are timed separately + reported together so CI logs
//! can attribute regressions. The single hard assertion is on the
//! aggregate: `init_ms + run_ms ≤ 60_000`.
//!
//! ## What's *not* measured
//!
//! - The interactive CLI prompt flow (`setup_vault` reads stdin).
//!   This test uses the programmatic equivalent — `Vault::create`
//!   directly with a fixed passphrase — because CI can't supply
//!   interactive input. The interactive cost is bounded separately
//!   by `rpassword` (read latency) + the Argon2id cost this test
//!   already covers.
//! - The connector marketplace fetch path (community connectors
//!   download). That's per-user, not part of the §16.4 budget.
//! - Compile time. CI's `cargo build --release` step is its own
//!   budget, separate from this runtime measurement.
//!
//! ## When this fires
//!
//! Run via `cargo test -p springtale-cli --test init_run_timing
//! --release`. The release profile is required — Argon2id in debug
//! mode is ~10× slower and would dominate the budget, hiding real
//! regressions in the other init steps.
//!
//! On regression: the test panics with a per-phase breakdown so the
//! offending step is immediately visible.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Instant;

use springtale_crypto::vault::store::Vault;
use springtale_runtime::config::{CooperationConfig, RuntimeConfig, StoreConfig};
use springtale_runtime::init;
use tempfile::TempDir;

/// §16.4 budget — fresh install to first running daemon in ≤60s.
const TIMING_BUDGET_MS: u128 = 60_000;

/// Stable passphrase used across the test so timing is reproducible.
/// Not a security boundary — the vault is created inside a `TempDir`
/// that's dropped at test exit.
const TEST_PASSPHRASE: &[u8] = b"timing-test-passphrase-not-a-secret";

#[tokio::test(flavor = "multi_thread")]
async fn init_and_run_completes_within_budget() {
    // Fresh temp dir — every run starts from "no springtale ever ran here."
    let temp = TempDir::new().expect("create temp dir");
    let vault_path = temp.path().join("vault.bin");
    let db_path = temp.path().join("springtale.db");

    // ── Phase 1: init ────────────────────────────────────────────
    let init_start = Instant::now();

    // Vault creation — Argon2id KDF is the slow step here. The
    // programmatic API mirrors what `setup_vault` does after reading
    // the passphrase interactively.
    let vault = Vault::create(&vault_path, TEST_PASSPHRASE).expect("vault create");
    vault.save().expect("vault save");

    // Bootstrap config — just paths + flags, no I/O cost worth
    // measuring (a few microseconds).
    let runtime_config = RuntimeConfig {
        store: StoreConfig {
            path: db_path.clone(),
            ephemeral: false,
            // Real production runs derive this from the passphrase
            // via `derive_db_encryption_key_hex`. We pass `None` here
            // because SQLCipher key derivation is a separate
            // measurement and would distort the runtime-init number.
            encryption_key_hex: None,
            retention_days: None,
        },
        ai_ollama: None,
        ai_openai: None,
        ai_anthropic: None,
        sentinel: None,
        connector_configs: Default::default(),
        cooperation: CooperationConfig::default(),
    };

    let init_ms = init_start.elapsed().as_millis();

    // ── Phase 2: run (runtime spin-up) ───────────────────────────
    let run_start = Instant::now();

    let (formation_cmd_tx, _formation_cmd_rx) = tokio::sync::mpsc::channel(32);
    let runtime = init::init(&runtime_config, formation_cmd_tx, None)
        .await
        .expect("runtime init");

    // Touch a field so the compiler doesn't optimise away the result.
    // The runtime construct itself is what we're timing; field access
    // is a no-op.
    assert!(runtime.canvas_tx.receiver_count() < usize::MAX);

    let run_ms = run_start.elapsed().as_millis();

    let total_ms = init_ms + run_ms;
    println!(
        "[init_run_timing] init: {init_ms} ms, run: {run_ms} ms, total: {total_ms} ms (budget: {TIMING_BUDGET_MS} ms)",
    );

    assert!(
        total_ms <= TIMING_BUDGET_MS,
        "fresh install to first running daemon exceeded {TIMING_BUDGET_MS}ms budget: \
         init={init_ms}ms, run={run_ms}ms, total={total_ms}ms. \
         See `COOPERATION_IMPLEMENTATION_PLAN.md §16 criterion 4` for the contract.",
    );
}
