use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::output;

/// Set up a duress passphrase for an existing vault.
///
/// Converts a legacy single-region vault to dual-region format.
/// The real passphrase opens the full vault data.
/// The duress passphrase opens a decoy profile with minimal data.
///
/// Both passphrases are required during setup. After setup, only
/// one passphrase is needed to open the vault — the system cannot
/// tell which one was used.
pub fn duress_setup(vault_path: &Path, json_out: bool) -> Result<()> {
    if !vault_path.exists() {
        anyhow::bail!(
            "vault file not found: {}. Run `springtale init` first.",
            vault_path.display()
        );
    }

    // Step 1: Verify real passphrase by opening existing vault
    let real_passphrase = rpassword::read_password_from_tty(Some("Current vault passphrase: "))
        .context("failed to read passphrase")?;

    let vault = springtale_crypto::vault::Vault::open(vault_path, real_passphrase.as_bytes())
        .context("failed to open vault — wrong passphrase?")?;

    // Get current entries
    let real_entries: HashMap<String, Vec<u8>> = vault
        .keys()
        .context("vault locked")?
        .into_iter()
        .filter_map(|k| vault.get(k).ok().flatten().map(|v| (k.clone(), v.clone())))
        .collect();

    // Step 2: Get duress passphrase
    let duress_passphrase = rpassword::read_password_from_tty(Some("New duress passphrase: "))
        .context("failed to read duress passphrase")?;

    if duress_passphrase.is_empty() {
        anyhow::bail!("duress passphrase cannot be empty");
    }

    if duress_passphrase == real_passphrase {
        anyhow::bail!("duress passphrase must be different from real passphrase");
    }

    let confirm = rpassword::read_password_from_tty(Some("Confirm duress passphrase: "))
        .context("failed to read confirmation")?;

    if duress_passphrase != confirm {
        anyhow::bail!("passphrases do not match");
    }

    // Step 3: Create minimal decoy entries
    let mut decoy_entries = HashMap::new();
    decoy_entries.insert("note".into(), b"Shopping list: milk, eggs, bread".to_vec());

    // Step 4: Write dual-region vault
    springtale_crypto::vault::duress::create_dual_vault(
        vault_path,
        real_passphrase.as_bytes(),
        duress_passphrase.as_bytes(),
        &real_entries,
        &decoy_entries,
    )
    .context("failed to create dual vault")?;

    let body = serde_json::json!({
        "duress_configured": true,
        "vault": vault_path.display().to_string(),
    });
    output::emit_status(json_out, &body, |_| {
        "Duress passphrase configured.\nReal passphrase → full access.\nDuress passphrase → decoy profile.\nFile size is constant — observer cannot tell which was used.".to_owned()
    })
}
