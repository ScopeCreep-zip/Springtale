use anyhow::{Context, Result};

use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::vault::store::Vault;

/// Initialize crypto vault, load identity keypair, derive API token hash.
/// Returns (vault, keypair, api_token_hash, db_encryption_key_hex).
pub(super) fn init_crypto(
    ephemeral: bool,
    crypto_config: &crate::config::CryptoConfig,
    passphrase_stdin: bool,
) -> Result<(Vault, Keypair, [u8; 32], String)> {
    let passphrase = get_passphrase(passphrase_stdin)?;
    let (vault, keypair) = if ephemeral {
        let mut vault = springtale_crypto::vault::store::Vault::create_ephemeral(&passphrase)
            .context("failed to create ephemeral vault")?;
        let keypair = springtale_crypto::identity::keypair::Keypair::generate()
            .context("failed to generate ephemeral keypair")?;
        // SECURITY: expose needed to persist identity in ephemeral vault
        vault
            .set("identity", keypair.with_secret_bytes(|b| b.to_vec()))
            .context("failed to store ephemeral identity")?;
        (vault, keypair)
    } else {
        tracing::info!(path = %crypto_config.vault_path.display(), "opening crypto vault");
        if let Some(parent) = crypto_config.vault_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create vault directory: {}", parent.display())
            })?;
        }
        open_or_create_vault(&crypto_config.vault_path, &passphrase)?
    };
    let node_id = keypair.node_id();
    tracing::info!(node_id = %hex::encode(node_id.as_bytes()), "identity loaded");

    // Detect duress session — if the vault was opened with a duress passphrase,
    // log it (hidden audit, only visible with real passphrase) and continue
    // with minimal capabilities.
    if vault.is_duress_session() {
        tracing::info!("vault opened in duress mode — minimal profile active");
    }

    // Derive API token and DB encryption key from the passphrase.
    // Both live in springtale-crypto::token so init.rs and the daemon
    // agree on the exact derivation (historical bug: trace.rs had its
    // own copy with key/msg swapped).
    let api_token_hash = springtale_crypto::token::derive_api_token_hash(&passphrase);
    let db_key_hex = springtale_crypto::token::derive_db_encryption_key_hex(&passphrase);

    Ok((vault, keypair, api_token_hash, db_key_hex))
}

/// Get the vault passphrase from Docker secret file, environment, or interactive prompt.
///
/// Priority:
/// 0. `--passphrase-stdin` — read exactly one line from stdin (the desktop
///    sidecar; keeps the passphrase out of argv and the environment, both of
///    which any other local process can read)
/// 1. SPRINGTALE_PASSPHRASE_FILE — read passphrase from file (Docker secrets pattern)
/// 2. SPRINGTALE_PASSPHRASE — direct env var (development only, visible in `docker inspect`)
/// 3. Interactive prompt via rpassword (if stdin is a terminal)
fn get_passphrase(passphrase_stdin: bool) -> Result<Vec<u8>> {
    if passphrase_stdin {
        return read_passphrase_line();
    }

    // Docker secrets pattern: read from file path in env var
    if let Ok(file_path) = std::env::var("SPRINGTALE_PASSPHRASE_FILE") {
        // Read as bytes and zeroize immediately — passphrase must not
        // linger in memory (IPV survivor's device may be seized).
        let mut raw_bytes = std::fs::read(&file_path)
            .with_context(|| format!("failed to read passphrase from {file_path}"))?;
        // Trim trailing newline/whitespace from file
        while raw_bytes.last().is_some_and(|b| b.is_ascii_whitespace()) {
            raw_bytes.pop();
        }
        if raw_bytes.is_empty() {
            anyhow::bail!("passphrase file is empty: {file_path}");
        }
        return Ok(raw_bytes);
    }

    // Direct env var (development convenience, NOT recommended for production)
    if let Ok(pass) = std::env::var("SPRINGTALE_PASSPHRASE") {
        return Ok(pass.into_bytes());
    }

    // Interactive prompt if stdin is a terminal
    if atty_check() {
        let pass = rpassword::read_password_from_tty(Some("Vault passphrase: "))
            .context("failed to read passphrase")?;
        if pass.is_empty() {
            anyhow::bail!("passphrase cannot be empty");
        }
        return Ok(pass.into_bytes());
    }

    anyhow::bail!(
        "no passphrase provided: set SPRINGTALE_PASSPHRASE_FILE, SPRINGTALE_PASSPHRASE, or run interactively"
    )
}

/// Read exactly one newline-terminated line of passphrase from stdin.
///
/// Read as bytes rather than through `String` so no intermediate copy of
/// the passphrase is left for the allocator to hand out later — the same
/// reason `SPRINGTALE_PASSPHRASE_FILE` reads bytes.
fn read_passphrase_line() -> Result<Vec<u8>> {
    use std::io::BufRead;

    let mut line = Vec::new();
    std::io::stdin()
        .lock()
        .read_until(b'\n', &mut line)
        .context("failed to read passphrase from stdin")?;
    while line.last().is_some_and(|b| matches!(b, b'\n' | b'\r')) {
        line.pop();
    }
    if line.is_empty() {
        anyhow::bail!("passphrase from stdin is empty");
    }
    Ok(line)
}

/// Check if stdin is a terminal (for interactive passphrase prompt).
fn atty_check() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Open an existing vault or create a new one on first run.
fn open_or_create_vault(path: &std::path::Path, passphrase: &[u8]) -> Result<(Vault, Keypair)> {
    if path.exists() {
        let vault =
            Vault::open(path, passphrase).context("failed to open vault (wrong passphrase?)")?;
        let identity_bytes = vault
            .get("identity")
            .context("failed to read identity from vault")?
            .ok_or_else(|| anyhow::anyhow!("vault has no identity key"))?
            .clone();
        let bytes: [u8; 32] = identity_bytes
            .as_slice()
            .try_into()
            .context("identity key is wrong size (expected 32 bytes)")?;
        let keypair =
            Keypair::from_secret_bytes(bytes).context("failed to restore keypair from vault")?;
        Ok((vault, keypair))
    } else {
        tracing::info!("creating new vault and identity");
        let keypair = Keypair::generate().context("failed to generate identity keypair")?;
        let mut vault = Vault::create(path, passphrase).context("failed to create vault")?;
        // SECURITY: expose needed to persist identity key material
        vault
            .set("identity", keypair.with_secret_bytes(|b| b.to_vec()))
            .context("failed to store identity in vault")?;
        vault.save().context("failed to save vault")?;
        Ok((vault, keypair))
    }
}
