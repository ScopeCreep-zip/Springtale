use std::path::PathBuf;

use anyhow::{Context, Result};

use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::vault::store::Vault;
use springtale_store::backend::sqlite::SqliteBackend;

/// Initialize Springtale: create data directory, vault, SQLite database, and default config.
pub async fn run() -> Result<()> {
    let data_dir = data_dir();
    println!("Initializing Springtale in {}", data_dir.display());

    // Create data directory
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create {}", data_dir.display()))?;
    println!("  Created {}", data_dir.display());

    // Create vault with passphrase
    let vault_path = data_dir.join("vault.bin");
    if vault_path.exists() {
        println!("  Vault already exists at {}", vault_path.display());
    } else {
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
        let mut vault = Vault::create(&vault_path, passphrase.as_bytes())
            .context("failed to create vault")?;
        // SECURITY: expose needed to persist identity key material
        vault
            .set("identity", keypair.expose_secret_bytes().to_vec())
            .context("failed to store identity")?;
        vault.save().context("failed to save vault")?;

        println!(
            "  Created vault at {}",
            vault_path.display()
        );
        println!(
            "  Generated identity: {}",
            hex::encode(keypair.node_id().as_bytes())
        );
    }

    // Create SQLite database
    let db_path = data_dir.join("springtale.db");
    if db_path.exists() {
        println!("  Database already exists at {}", db_path.display());
    } else {
        let _store = SqliteBackend::open(&db_path)
            .context("failed to create database")?;
        println!("  Created database at {}", db_path.display());
    }

    // Create default config file
    let config_path = PathBuf::from("springtale.toml");
    if config_path.exists() {
        println!("  Config already exists at {}", config_path.display());
    } else {
        let default_config = format!(
            r#"# Springtale configuration
# See docs/current-arch/ARCHITECTURE.md §8.1 for full reference.

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

        std::fs::write(&config_path, default_config)
            .context("failed to write springtale.toml")?;
        println!("  Created {}", config_path.display());
    }

    println!("\nSpringtale initialized. Run `springtale server start` to begin.");
    Ok(())
}

fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })
        .map(|base| base.join("springtale"))
        .unwrap_or_else(|| PathBuf::from(".springtale"))
}
