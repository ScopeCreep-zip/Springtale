use std::path::Path;

use anyhow::{Context, Result};

use springtale_store::backend::trait_::StorageBackend;

use crate::output;

/// Prepare for travel: export encrypted backup, then wipe local data.
///
/// 1. Prompts for a travel passphrase (separate from vault passphrase)
/// 2. Delegates to `springtale_runtime::operations::travel::prepare` which
///    exports encrypted backup and wipes all local data
///
/// The backup file is indistinguishable from random data without
/// the travel passphrase.
pub fn prepare(
    backup_path: &Path,
    vault_path: &Path,
    db_path: &Path,
    config_path: &Path,
    store: &dyn StorageBackend,
    json_out: bool,
) -> Result<()> {
    // Prompt for travel passphrase
    let passphrase = rpassword::read_password_from_tty(Some("Travel passphrase: "))
        .context("failed to read travel passphrase")?;

    if passphrase.is_empty() {
        anyhow::bail!("travel passphrase cannot be empty");
    }

    let confirm = rpassword::read_password_from_tty(Some("Confirm travel passphrase: "))
        .context("failed to read confirmation")?;

    if passphrase != confirm {
        anyhow::bail!("passphrases do not match");
    }

    springtale_runtime::operations::travel::prepare(
        vault_path,
        db_path,
        config_path,
        backup_path,
        passphrase.as_bytes(),
        store,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let body = serde_json::json!({
        "backup": backup_path.display().to_string(),
        "wiped": true,
    });
    output::emit_status(json_out, &body, |v| {
        format!(
            "Backup saved to: {}\nLocal data wiped. Safe travels.",
            output::cell(v, "backup")
        )
    })
}

/// Restore from an encrypted backup after travel.
///
/// 1. Prompts for the travel passphrase
/// 2. Delegates to `springtale_runtime::operations::travel::restore`
pub fn restore(
    backup_path: &Path,
    vault_path: &Path,
    db_path: &Path,
    config_path: &Path,
    json_out: bool,
) -> Result<()> {
    if !backup_path.exists() {
        anyhow::bail!("backup file not found: {}", backup_path.display());
    }

    let passphrase = rpassword::read_password_from_tty(Some("Travel passphrase: "))
        .context("failed to read travel passphrase")?;

    springtale_runtime::operations::travel::restore(
        backup_path,
        vault_path,
        db_path,
        config_path,
        passphrase.as_bytes(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let body = serde_json::json!({
        "restored": true,
        "backup": backup_path.display().to_string(),
    });
    output::emit_status(json_out, &body, |_| "Data restored from backup.".to_owned())
}
