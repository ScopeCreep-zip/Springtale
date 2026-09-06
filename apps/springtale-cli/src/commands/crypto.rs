use anyhow::{Context, Result};

use crate::output;

/// Re-encrypt the vault with a new passphrase.
///
/// Opens the vault with the current passphrase, reads all entries,
/// creates a new vault with the new passphrase, copies entries, and saves.
pub fn rotate_vault_key(json_out: bool) -> Result<()> {
    let vault_path = springtale_store::paths::default_vault_path();
    if !vault_path.exists() {
        anyhow::bail!("no vault found at {}", vault_path.display());
    }

    // Prompt for current passphrase
    let old_pass = rpassword::read_password_from_tty(Some("Current vault passphrase: "))
        .context("failed to read passphrase")?;
    if old_pass.is_empty() {
        anyhow::bail!("passphrase cannot be empty");
    }

    // Open vault with old passphrase
    let vault = springtale_crypto::vault::store::Vault::open(&vault_path, old_pass.as_bytes())
        .context("failed to open vault (wrong passphrase?)")?;

    // Prompt for new passphrase
    let new_pass = rpassword::read_password_from_tty(Some("New vault passphrase: "))
        .context("failed to read new passphrase")?;
    if new_pass.is_empty() {
        anyhow::bail!("new passphrase cannot be empty");
    }
    let confirm = rpassword::read_password_from_tty(Some("Confirm new passphrase: "))
        .context("failed to read confirmation")?;
    if new_pass != confirm {
        anyhow::bail!("passphrases do not match");
    }

    // Read all entries from old vault
    let keys = vault.keys().context("failed to list vault keys")?;

    // Collect entries before creating new vault (old vault consumed by move)
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for key in &keys {
        if let Ok(Some(value)) = vault.get(key) {
            entries.push(((*key).clone(), value.clone()));
        }
    }

    // Create new vault with new passphrase (overwrites path on save)
    let mut new_vault =
        springtale_crypto::vault::store::Vault::create(&vault_path, new_pass.as_bytes())
            .context("failed to create new vault")?;

    // Copy all entries to new vault
    for (key, value) in entries {
        new_vault
            .set(key, value)
            .context("failed to write entry to new vault")?;
    }

    new_vault.save().context("failed to save new vault")?;

    let body = serde_json::json!({ "rotated": true, "entries": keys.len() });
    output::emit_status(json_out, &body, |_| {
        "Vault key rotated successfully.".to_owned()
    })
}
