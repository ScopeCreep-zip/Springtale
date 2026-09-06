//! `springtale init` — thin wrapper around vault/DB/config setup.
//!
//! All wizard logic (platform list, field schemas, persistence) lives in
//! [`springtale_runtime::operations::onboarding`]. This file only:
//!   1. Creates the data directory, vault, and (encrypted) database.
//!   2. Writes a minimal bootstrap `springtale.toml` with NO secrets.
//!   3. Iterates the onboarding platform forms, prompts for each field,
//!      and calls `apply_platform` to persist answers into the encrypted
//!      database's config_store.
//!
//! The old version appended bot tokens to `springtale.toml` directly.
//! That's banned — secrets never land in user-editable TOML files.
//!
//! `--json` is deliberately not honoured here: `init` is an interactive
//! wizard whose stdout is prompts the user answers on stdin, not a result
//! document. Scripted setup goes through `springtale new <template>` plus
//! the daemon's config routes, both of which do honour the flag.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};

use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::token::derive_db_encryption_key_hex;
use springtale_crypto::vault::store::Vault;
use springtale_runtime::operations::onboarding::{self, FormField, PlatformForm};
use springtale_store::StorageBackend;
use springtale_store::backend::sqlite::SqliteBackend;

pub async fn run() -> Result<()> {
    let data_dir = springtale_store::paths::data_dir();
    println!("Initializing Springtale in {}", data_dir.display());

    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create {}", data_dir.display()))?;
    println!("  Created {}", data_dir.display());

    let vault_path = data_dir.join("vault.bin");
    let db_path = data_dir.join("springtale.db");

    let passphrase = setup_vault(&vault_path)?;
    setup_database(&db_path, &passphrase)?;
    write_bootstrap_config(&db_path, &vault_path, &data_dir)?;

    let db_key_hex = derive_db_encryption_key_hex(&passphrase);
    let store = SqliteBackend::open_encrypted(&db_path, &db_key_hex)
        .context("failed to open encrypted database for onboarding")?;

    run_platform_wizard(&store).await?;
    set_owner_id(&store).await?;

    println!("\nSpringtale initialized. Run `springtale server start` to begin.");
    println!("Or try: springtale new telegram-bot");
    Ok(())
}

/// Prompt for the bot owner's platform user ID so the pairing gate
/// knows who to trust on first boot. Without this, preconfigured mode
/// (the default) denies all messages.
async fn set_owner_id(store: &SqliteBackend) -> Result<()> {
    println!("\n--- Owner Setup ---");
    println!("Enter your user ID on the chat platform (e.g., your Telegram numeric ID).");
    println!("This user will be the bot owner — they can manage pairing and settings.");
    println!(
        "(Leave blank to use trust-on-first-use mode — first person to message becomes owner)"
    );
    print!("> ");
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read owner ID")?;
    let input = input.trim();

    if input.is_empty() {
        store
            .set_config("bot:access_mode", "\"tofu\"")
            .await
            .context("failed to set access mode")?;
        println!("  TOFU mode enabled — first user to message becomes owner.");
    } else {
        let owner_val = format!("\"{input}\"");
        store
            .set_config("bot:owner_id", &owner_val)
            .await
            .context("failed to set owner ID")?;
        println!("  Owner set to: {input}");
    }
    Ok(())
}

/// Open or create the vault. Returns the passphrase bytes so the caller
/// can derive the database key.
fn setup_vault(vault_path: &std::path::Path) -> Result<Vec<u8>> {
    if vault_path.exists() {
        println!("  Vault already exists at {}", vault_path.display());
        let passphrase = rpassword::read_password_from_tty(Some("Vault passphrase: "))
            .context("failed to read passphrase")?;
        if passphrase.is_empty() {
            anyhow::bail!("passphrase cannot be empty");
        }
        // Verify by attempting to open
        let _ = Vault::open(vault_path, passphrase.as_bytes())
            .context("failed to open vault (wrong passphrase?)")?;
        return Ok(passphrase.into_bytes());
    }

    let passphrase = rpassword::read_password_from_tty(Some("Choose a vault passphrase: "))
        .context("failed to read passphrase")?;
    if passphrase.is_empty() {
        anyhow::bail!("passphrase cannot be empty");
    }
    let confirm = rpassword::read_password_from_tty(Some("Confirm passphrase: "))
        .context("failed to read confirmation")?;
    if passphrase != confirm {
        anyhow::bail!("passphrases do not match");
    }

    let keypair = Keypair::generate().context("failed to generate identity")?;
    let mut vault =
        Vault::create(vault_path, passphrase.as_bytes()).context("failed to create vault")?;
    vault
        .set("identity", keypair.with_secret_bytes(|b| b.to_vec()))
        .context("failed to store identity")?;
    vault.save().context("failed to save vault")?;

    println!("  Created vault at {}", vault_path.display());
    println!(
        "  Generated identity: {}",
        hex::encode(keypair.node_id().as_bytes())
    );
    Ok(passphrase.into_bytes())
}

fn setup_database(db_path: &std::path::Path, passphrase: &[u8]) -> Result<()> {
    if db_path.exists() {
        println!("  Database already exists at {}", db_path.display());
        return Ok(());
    }
    let key_hex = derive_db_encryption_key_hex(passphrase);
    let _store = SqliteBackend::open_encrypted(db_path, &key_hex)
        .context("failed to create encrypted database")?;
    println!("  Created database at {}", db_path.display());
    Ok(())
}

/// Write a minimal bootstrap `springtale.toml` with NO secrets — just
/// paths the daemon needs to find the vault, DB, and socket. Every
/// connector credential lives in the encrypted config_store via the
/// onboarding wizard.
fn write_bootstrap_config(
    db_path: &std::path::Path,
    vault_path: &std::path::Path,
    data_dir: &std::path::Path,
) -> Result<()> {
    let config_path = PathBuf::from("springtale.toml");
    if config_path.exists() {
        println!("  Config already exists at {}", config_path.display());
        return Ok(());
    }
    let default_config = format!(
        r#"# Springtale bootstrap configuration.
#
# This file holds ONLY non-secret paths. Connector credentials (bot tokens,
# API keys, passphrases) are stored in the encrypted database via
# `springtale init` or the dashboard — NEVER add them to this file.
#
# Reference: docs/current-arch/ARCHITECTURE.md §8.1

[store]
path = "{db_path}"

[crypto]
vault_path = "{vault_path}"

[transport]
socket_path = "{socket_path}"

[api]
bind = "127.0.0.1:8080"
"#,
        db_path = db_path.display(),
        vault_path = vault_path.display(),
        socket_path = data_dir.join("springtale.sock").display(),
    );

    std::fs::write(&config_path, default_config).context("failed to write springtale.toml")?;
    println!("  Created {}", config_path.display());
    Ok(())
}

/// Iterate onboarding platforms and persist any the user wants to set up.
async fn run_platform_wizard(store: &SqliteBackend) -> Result<()> {
    println!("\n--- Channel Setup ---");
    let platforms = onboarding::list_platforms();
    let choices: Vec<&str> = platforms.iter().map(|p| p.id).collect();
    println!(
        "Connect a chat platform? Options: {}, skip",
        choices.join(", ")
    );
    print!("> ");
    io::stdout().flush().ok();

    let mut selection = String::new();
    io::stdin()
        .read_line(&mut selection)
        .context("failed to read input")?;
    let selection = selection.trim().to_lowercase();

    if selection.is_empty() || selection == "skip" {
        println!("  Skipped channel setup. Run `springtale init` again to add one later.");
        return Ok(());
    }

    let Some(platform) = onboarding::get_platform(&selection) else {
        println!("  Unknown platform: {selection}. Skipping channel setup.");
        return Ok(());
    };

    println!("\n{}", platform.label);
    println!("{}", platform.setup_help);

    let answers = prompt_platform(platform)?;
    let report = onboarding::apply_platform(store, platform.id, answers)
        .await
        .with_context(|| format!("failed to apply {} setup", platform.id))?;
    println!(
        "  {} configured. Stored {} field{} at {}.",
        platform.label,
        report.fields_stored.len(),
        if report.fields_stored.len() == 1 {
            ""
        } else {
            "s"
        },
        report.stored_key,
    );
    Ok(())
}

/// Prompt interactively for each field on a platform form.
fn prompt_platform(platform: &PlatformForm) -> Result<BTreeMap<String, String>> {
    let mut answers = BTreeMap::new();
    for field in platform.fields {
        let value = prompt_field(field)?;
        if !value.is_empty() {
            answers.insert(field.name.to_owned(), value);
        }
    }
    Ok(answers)
}

fn prompt_field(field: &FormField) -> Result<String> {
    let label = match field.default {
        Some(default) => format!("{} [{}]: ", field.label, default),
        None => format!("{}: ", field.label),
    };

    if field.secret {
        let value = rpassword::read_password_from_tty(Some(&label))
            .with_context(|| format!("failed to read {}", field.name))?;
        Ok(value)
    } else {
        print!("{label}");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .with_context(|| format!("failed to read {}", field.name))?;
        Ok(input.trim().to_owned())
    }
}
