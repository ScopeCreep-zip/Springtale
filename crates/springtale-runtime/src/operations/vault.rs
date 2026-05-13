//! Vault operations — create, open, lock, status.

use std::path::Path;

use specta::Type;
use serde::Serialize;

use springtale_crypto::vault::store::Vault;

use crate::error::OperationError;

/// Vault status returned to callers.
#[derive(Debug, Serialize, Type)]
pub struct VaultStatus {
    pub unlocked: bool,
    pub duress_session: bool,
}

/// Create a new vault with passphrase + identity keypair.
///
/// Reuses logic from springtale-cli init command.
pub fn create_vault(
    vault_path: &Path,
    passphrase: &[u8],
) -> Result<(Vault, VaultStatus), OperationError> {
    if vault_path.exists() {
        return Err(OperationError::Validation(
            "vault already exists — unlock it instead".into(),
        ));
    }

    if let Some(parent) = vault_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| OperationError::Rule(format!("failed to create directory: {e}")))?;
    }

    let mut vault = Vault::create(vault_path, passphrase)
        .map_err(|e| OperationError::Rule(format!("failed to create vault: {e}")))?;

    let keypair = springtale_crypto::identity::keypair::Keypair::generate()
        .map_err(|e| OperationError::Rule(format!("failed to generate identity: {e}")))?;

    // SECURITY: expose needed to persist identity key material
    vault
        .set("identity", keypair.expose_secret_bytes().to_vec())
        .map_err(|e| OperationError::Rule(format!("failed to store identity: {e}")))?;

    vault
        .save()
        .map_err(|e| OperationError::Rule(format!("failed to save vault: {e}")))?;

    let status = VaultStatus {
        unlocked: vault.is_unlocked(),
        duress_session: vault.is_duress_session(),
    };

    Ok((vault, status))
}

/// Open an existing vault with passphrase.
pub fn open_vault(
    vault_path: &Path,
    passphrase: &[u8],
) -> Result<(Vault, VaultStatus), OperationError> {
    if !vault_path.exists() {
        return Err(OperationError::NotFound(
            "vault file not found — create a vault first".into(),
        ));
    }

    let vault = Vault::open(vault_path, passphrase).map_err(|e| match e {
        springtale_crypto::error::CryptoError::VaultDecryptionFailed => {
            OperationError::Validation("wrong passphrase or corrupted vault".into())
        }
        springtale_crypto::error::CryptoError::InsecurePermissions => OperationError::Validation(
            "vault file has insecure permissions (should be 0600)".into(),
        ),
        _ => OperationError::Rule(format!("failed to open vault: {e}")),
    })?;

    let status = VaultStatus {
        unlocked: vault.is_unlocked(),
        duress_session: vault.is_duress_session(),
    };

    Ok((vault, status))
}

/// Lock the vault — zeroes key material.
pub fn lock_vault(vault: &mut Vault) {
    vault.lock();
}

/// Get vault status without modifying it.
pub fn get_vault_status(vault: &Option<Vault>) -> VaultStatus {
    match vault {
        Some(v) => VaultStatus {
            unlocked: v.is_unlocked(),
            duress_session: v.is_duress_session(),
        },
        None => VaultStatus {
            unlocked: false,
            duress_session: false,
        },
    }
}
