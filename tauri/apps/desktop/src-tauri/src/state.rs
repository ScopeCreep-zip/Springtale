//! Shared desktop application state.
//!
//! Plan 2.1: `springtaled` is the only state owner and the desktop shell
//! is a sidecar client. Nothing here holds a runtime, a scheduler, a bot
//! or a store — those live in the daemon process and are reached over its
//! HTTP API. What remains is the state the OS window genuinely owns: the
//! vault the passphrase overlay manages, the auto-lock timer, and the
//! `{ port, token }` needed to talk to the daemon we started.

use std::sync::Arc;

use tokio::sync::Mutex;

use springtale_crypto::vault::store::Vault;

use crate::autolock::AutoLockHandle;
use crate::sidecar::Daemon;

/// A running daemon plus the bearer token the frontend authenticates with.
///
/// The token is one the daemon issued at `POST /auth/login` (plan 6.6):
/// 32 random bytes, held here for the life of the unlocked session and
/// dropped when the vault locks. Nothing derives it from the passphrase.
pub struct DaemonHandle {
    /// Loopback port the daemon's management API is listening on.
    pub port: u16,
    /// Hex-encoded API bearer token.
    pub token: String,
    /// Child process handle, so `lock_vault` can terminate the daemon.
    pub child: tauri_plugin_shell::process::CommandChild,
}

impl DaemonHandle {
    /// Build a handle from a spawned daemon and the derived token.
    #[must_use]
    pub fn new(daemon: Daemon, token: String) -> Self {
        Self {
            port: daemon.port,
            token,
            child: daemon.child,
        }
    }
}

/// Shared application state for Tauri commands.
///
/// The window opens instantly with everything empty; the passphrase
/// overlay populates `vault` and `daemon` on unlock.
pub struct AppState {
    /// Vault — managed via UI (user types passphrase).
    pub vault: Arc<Mutex<Option<Vault>>>,
    /// Auto-lock timer (Rust backend, not JS).
    pub auto_lock: Arc<Mutex<AutoLockHandle>>,
    /// The `springtaled` sidecar — `None` while the vault is locked.
    pub daemon: Arc<Mutex<Option<DaemonHandle>>>,
}

impl AppState {
    /// Create the app shell — instant, no disk access, no passphrase.
    #[must_use]
    pub fn shell() -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
            auto_lock: Arc::new(Mutex::new(AutoLockHandle::new())),
            daemon: Arc::new(Mutex::new(None)),
        }
    }
}
