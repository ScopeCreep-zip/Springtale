//! Auto-lock timer — locks the vault after inactivity.
//!
//! Per ARCHITECTURE.md §2.7: "Vault auto-locks after configurable
//! inactivity (default: 5 min). Tauri modal in 2b."
//!
//! Runs as a tokio task in the Rust backend — NOT in frontend JS.
//! If the WebView freezes or crashes, the timer still fires and
//! locks the vault. Fail-safe by design.

use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::Mutex;

use springtale_crypto::vault::store::Vault;

/// Handle to a running auto-lock timer. Reset on user activity.
pub struct AutoLockHandle {
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl AutoLockHandle {
    pub fn new() -> Self {
        Self { cancel_tx: None }
    }

    /// Reset the auto-lock timer.
    ///
    /// Cancels any existing timer and starts a new one. If `timeout_minutes`
    /// is 0, auto-lock is disabled (no timer started).
    ///
    /// When the timer fires without being reset:
    /// 1. Vault key material is zeroed (`vault.lock()`)
    /// 2. Vault is removed from AppState
    /// 3. Frontend is notified via "vault-locked" event
    pub fn reset(
        &mut self,
        timeout_minutes: u32,
        vault: Arc<Mutex<Option<Vault>>>,
        app_handle: tauri::AppHandle,
    ) {
        // Cancel existing timer
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }

        if timeout_minutes == 0 {
            return; // disabled
        }

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        self.cancel_tx = Some(cancel_tx);

        let duration = std::time::Duration::from_secs(u64::from(timeout_minutes) * 60);

        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(duration) => {
                    // Timer fired — lock the vault
                    let mut guard = vault.lock().await;
                    if let Some(ref mut v) = *guard {
                        v.lock(); // zeroes key material
                    }
                    *guard = None;
                    tracing::info!("auto-lock: vault locked after inactivity");
                    let _ = app_handle.emit("vault-locked", ());
                }
                _ = cancel_rx => {
                    // Timer cancelled — user was active
                }
            }
        });
    }
}
