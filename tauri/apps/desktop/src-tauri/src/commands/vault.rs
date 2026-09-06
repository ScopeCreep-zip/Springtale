//! Vault overlay + daemon lifecycle.
//!
//! Plan 2.1: unlocking the vault is what starts `springtaled`. The shell
//! opens the vault file itself only to fail fast on a wrong passphrase and
//! to create the identity on first run; the daemon then opens the same
//! file and owns everything downstream of it.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tauri_specta::Event;

use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::vault::store::Vault;

use crate::paths::default_vault_path;
use crate::sidecar;
use crate::state::{AppState, DaemonHandle};

/// Emitted after a successful `create_vault` or `unlock_vault` so the
/// frontend can close the passphrase overlay and start loading data.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct VaultUnlocked;

/// Emitted after `lock_vault` and from the autolock timer when the
/// inactivity threshold expires. Unit payload; the frontend zeroizes
/// any in-memory secrets and re-shows the passphrase overlay.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct VaultLocked;

/// Vault status returned to the frontend.
#[derive(Debug, Clone, Serialize, Type)]
pub struct VaultStatus {
    /// Whether a vault is currently open in this process.
    pub unlocked: bool,
    /// Whether the open vault was unlocked with a duress passphrase.
    pub duress_session: bool,
}

/// An unlocked vault plus the sidecar it started.
///
/// The frontend points its HTTP provider at `http://127.0.0.1:{port}`
/// with `Authorization: Bearer {token}`.
#[derive(Debug, Clone, Serialize, Type)]
pub struct VaultSession {
    /// Vault status after the unlock.
    pub status: VaultStatus,
    /// Loopback port the daemon's management API bound.
    pub port: u16,
    /// Hex-encoded API bearer token.
    pub token: String,
}

/// Create a new vault with a passphrase, then start the daemon on it.
#[tauri::command]
#[specta::specta]
pub async fn create_vault(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    passphrase: String,
) -> Result<VaultSession, String> {
    let vault_path = default_vault_path();
    if vault_path.exists() {
        return Err("vault already exists — unlock it instead".to_owned());
    }
    if let Some(parent) = vault_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create directory: {e}"))?;
    }

    let mut vault = Vault::create(&vault_path, passphrase.as_bytes()).map_err(|e| e.to_string())?;
    let keypair = Keypair::generate().map_err(|e| e.to_string())?;
    // SECURITY: expose needed to persist the identity key inside the vault
    vault
        .set("identity", keypair.with_secret_bytes(|b| b.to_vec()))
        .map_err(|e| e.to_string())?;
    vault.save().map_err(|e| e.to_string())?;

    start_session(&state, &app, vault, SecretString::from(passphrase)).await
}

/// Unlock the vault with a passphrase, then start the daemon on it.
#[tauri::command]
#[specta::specta]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    passphrase: String,
) -> Result<VaultSession, String> {
    let vault_path = default_vault_path();
    let vault = Vault::open(&vault_path, passphrase.as_bytes())
        .map_err(|_| "failed to open vault (wrong passphrase?)".to_owned())?;

    start_session(&state, &app, vault, SecretString::from(passphrase)).await
}

/// Shared tail of create/unlock: derive the API token, start the sidecar,
/// publish the session.
///
/// If a daemon is already running (repeated unlock attempts before the
/// overlay closes) the existing one is reused rather than a second copy
/// spawned — one state owner, always.
async fn start_session(
    state: &State<'_, AppState>,
    app: &tauri::AppHandle,
    vault: Vault,
    passphrase: SecretString,
) -> Result<VaultSession, String> {
    let status = VaultStatus {
        unlocked: true,
        duress_session: vault.is_duress_session(),
    };

    let mut daemon_guard = state.daemon.lock().await;
    if let Some(existing) = daemon_guard.as_ref() {
        let session = VaultSession {
            status,
            port: existing.port,
            token: existing.token.clone(),
        };
        drop(daemon_guard);
        *state.vault.lock().await = Some(vault);
        let _ = VaultUnlocked.emit(app);
        return Ok(session);
    }

    // The daemon derives the same value from the same passphrase in
    // `runtime/boot/crypto.rs`; it is never sent over the pipe.
    // SECURITY: expose needed to derive the API token hash the daemon
    // independently derives from the same passphrase.
    let token = {
        use secrecy::ExposeSecret as _;
        hex::encode(springtale_crypto::token::derive_api_token_hash(
            passphrase.expose_secret().as_bytes(),
        ))
    };

    let daemon = sidecar::start(app, &passphrase).await?;
    let session = VaultSession {
        status,
        port: daemon.port,
        token: token.clone(),
    };
    *daemon_guard = Some(DaemonHandle::new(daemon, token));
    drop(daemon_guard);

    *state.vault.lock().await = Some(vault);
    let _ = VaultUnlocked.emit(app);
    Ok(session)
}

/// Lock the vault — drops key material and stops the daemon, so no
/// database access is possible until the next unlock.
#[tauri::command]
#[specta::specta]
pub async fn lock_vault(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    // `Vault` zeroizes its key material on drop.
    *state.vault.lock().await = None;

    if let Some(daemon) = state.daemon.lock().await.take()
        && let Err(e) = daemon.child.kill()
    {
        tracing::warn!(error = %e, "failed to stop springtaled sidecar");
    }

    let _ = VaultLocked.emit(&app);
    Ok(())
}

/// Get the current vault status.
#[tauri::command]
#[specta::specta]
pub async fn get_vault_status(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    let guard = state.vault.lock().await;
    Ok(match guard.as_ref() {
        Some(vault) => VaultStatus {
            unlocked: true,
            duress_session: vault.is_duress_session(),
        },
        None => VaultStatus {
            unlocked: false,
            duress_session: false,
        },
    })
}
