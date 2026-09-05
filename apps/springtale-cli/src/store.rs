//! Opening the encrypted store from the CLI (plan 0.5).
//!
//! There is no plaintext store. The passphrase comes from, in order: a
//! passphrase file readable only by its owner (restic `--password-file`),
//! a command that prints it (borg `BORG_PASSCOMMAND`, so the user can
//! plug in their own keychain), or an interactive prompt. Never an
//! environment variable: those leak through `/proc/self/environ`, logs
//! and core dumps (OWASP Secrets Management cheat sheet).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use secrecy::{ExposeSecret, SecretString};

use springtale_crypto::token::derive_db_encryption_key_hex;
use springtale_store::backend::sqlite::SqliteBackend;

/// Where the CLI gets the vault passphrase for commands that open the store.
#[derive(Debug, Clone, Default)]
pub struct PassphraseOpts {
    pub passphrase_file: Option<PathBuf>,
    pub passphrase_command: Option<String>,
}

/// Open the encrypted store at the configured path.
pub fn open_store(opts: &PassphraseOpts) -> Result<SqliteBackend> {
    let db_path = store_path_from_config();
    if !db_path.exists() {
        bail!(
            "database not found at {}. Run `springtale init` first.",
            db_path.display()
        );
    }

    let key_hex = derive_db_key_hex(opts)?;
    SqliteBackend::open_encrypted(&db_path, &key_hex)
        .context("failed to open encrypted database (wrong passphrase?)")
}

/// Read the passphrase (file, command, or prompt) and derive the hex
/// store key from it, for callers that hand the key to an operation
/// instead of opening the store themselves (`doctor`, `fix`).
pub fn derive_db_key_hex(opts: &PassphraseOpts) -> Result<String> {
    let passphrase = read_passphrase(opts)?;
    // SECURITY: expose needed to derive the DB key; the derived hex is never logged
    Ok(derive_db_encryption_key_hex(
        passphrase.expose_secret().as_bytes(),
    ))
}

fn read_passphrase(opts: &PassphraseOpts) -> Result<SecretString> {
    if let Some(file) = &opts.passphrase_file {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            if meta.permissions().mode() & 0o077 != 0 {
                bail!(
                    "{} must be readable only by you (chmod 600)",
                    file.display()
                );
            }
        }
        let text = std::fs::read_to_string(file)
            .with_context(|| format!("cannot read passphrase file {}", file.display()))?;
        return Ok(text.trim_end().to_owned().into());
    }

    if let Some(cmd) = &opts.passphrase_command {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .context("passphrase command failed to run")?;
        if !out.status.success() {
            bail!("passphrase command exited with {}", out.status);
        }
        let text =
            String::from_utf8(out.stdout).context("passphrase command printed invalid UTF-8")?;
        return Ok(text.trim_end().to_owned().into());
    }

    let text = rpassword::read_password_from_tty(Some("Vault passphrase: "))
        .context("failed to read passphrase")?;
    Ok(text.into())
}

/// The store path from `springtale.toml` (or the platform default).
fn store_path_from_config() -> PathBuf {
    let config_path = "springtale.toml";
    if !std::path::Path::new(config_path).exists() {
        return springtale_store::paths::default_db_path();
    }

    // Parse just the store section from config.
    let figment = figment::Figment::new()
        .merge(<figment::providers::Toml as figment::providers::Format>::file(config_path))
        .merge(
            figment::providers::Env::prefixed("SPRINGTALE_")
                .map(|key| key.as_str().replace("__", ".").into()),
        );

    #[derive(serde::Deserialize, Default)]
    struct PartialConfig {
        #[serde(default)]
        store: StoreSection,
    }
    #[derive(serde::Deserialize, Default)]
    struct StoreSection {
        #[serde(default = "springtale_store::paths::default_db_path")]
        path: PathBuf,
    }

    let config: PartialConfig = figment.extract().unwrap_or_default();
    config.store.path
}
