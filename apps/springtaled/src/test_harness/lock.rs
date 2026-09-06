//! A [`RuntimeGuard`] over the in-memory fixture, for lock/unlock tests.
//!
//! Locking the real daemon drops a runtime built on an encrypted SQLite
//! file and a vault on disk. Rebuilding one of those per test would make
//! the suite slow and would test Argon2id rather than the lock. This
//! fixture keeps the lock machinery exactly as it ships — the same
//! [`RuntimeGuard`], the same outer router, the same teardown — and
//! swaps only the pipeline behind `POST /vault/unlock` for one that
//! rebuilds a [`TestApp`] in memory.

use std::sync::{Arc, Mutex};

use axum::Router;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc;

use springtale_cooperation::command::FormationCommand;
use springtale_crypto::vault::store::Vault;
use springtale_runtime::TaskHandles;

use crate::api::lock::{Live, RebuildFuture, RuntimeGuard, build_outer_router};

use super::app::TestApp;

/// The passphrase the fixture's vault and API token are derived from.
/// Matches [`TestApp::build`], which derives its token from the same
/// bytes.
pub const TEST_PASSPHRASE: &str = "test-passphrase";

/// A locked-daemon fixture: the outer router, the guard behind it, and
/// the token the inner router accepts.
pub struct TestGuard {
    /// The outer router — three always-on routes plus the forwarding
    /// fallback. Exactly what `boot` serves.
    pub router: Router,
    /// The guard, for asserting on lock state directly.
    pub guard: RuntimeGuard,
    /// Hex-encoded API token accepted as `Authorization: Bearer …`.
    pub token_hex: String,
    /// Formation receivers for every world built so far. Held open so
    /// `formation_cmd_tx.send` keeps succeeding after an unlock.
    _formation_rx: Arc<Mutex<Vec<mpsc::Receiver<FormationCommand>>>>,
}

impl TestGuard {
    /// Build an unlocked fixture.
    ///
    /// Must be called from inside a Tokio runtime — `TestApp::build`
    /// registers a filesystem watcher with it.
    pub fn build() -> Self {
        let held: Arc<Mutex<Vec<mpsc::Receiver<FormationCommand>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let token_hex = TestApp::build(true).token_hex;

        let live = build_test_live(&held);
        let rebuild_held = held.clone();
        let rebuild: crate::api::lock::Rebuild =
            Arc::new(move |passphrase: SecretString| -> RebuildFuture {
                let held = rebuild_held.clone();
                Box::pin(async move {
                    // Stands in for `Vault::open`, which is the real
                    // passphrase check: a wrong passphrase fails AEAD
                    // decryption and no runtime is built.
                    // SECURITY: expose needed to compare against the
                    // fixture passphrase; the borrow does not escape.
                    if passphrase.expose_secret() != TEST_PASSPHRASE {
                        anyhow::bail!("wrong passphrase or corrupted vault");
                    }
                    Ok(build_test_live(&held))
                })
            });

        let guard = RuntimeGuard::new(live, rebuild);
        Self {
            router: build_outer_router(guard.clone()),
            guard,
            token_hex,
            _formation_rx: held,
        }
    }
}

/// One in-memory world, with its formation receiver parked in `held`.
fn build_test_live(held: &Arc<Mutex<Vec<mpsc::Receiver<FormationCommand>>>>) -> Live {
    let TestApp {
        state,
        formation_cmd_rx,
        ..
    } = TestApp::build(true);
    match held.lock() {
        Ok(mut list) => list.push(formation_cmd_rx),
        Err(poisoned) => poisoned.into_inner().push(formation_cmd_rx),
    }
    let vault = Vault::create_ephemeral(TEST_PASSPHRASE.as_bytes()).expect("ephemeral vault");
    Live::new(state, TaskHandles::new(), vault, None)
}
