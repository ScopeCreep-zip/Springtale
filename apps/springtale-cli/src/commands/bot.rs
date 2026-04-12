//! `springtale bot` subcommands — pairing management from the daemon host.
//!
//! These commands run on the trusted device (the server), never via chat.
//! They open the encrypted database directly so they work without the
//! daemon running — critical for the `panic-unpair` IPV scenario.

use anyhow::{Context, Result};

use springtale_crypto::token::derive_db_encryption_key_hex;
use springtale_runtime::operations::pairing;
use springtale_store::backend::sqlite::SqliteBackend;

pub async fn pair_init() -> Result<()> {
    let store = open_store()?;
    let code = pairing::generate_pairing_code(&store)
        .await
        .context("failed to generate pairing code")?;

    println!("Pairing code (give this to the user, do NOT send via chat):\n");
    println!("  {code}\n");
    println!("The user types this code into their chat with the bot.");
    println!("Code expires in 10 minutes. Single-use.");
    Ok(())
}

pub async fn panic_unpair() -> Result<()> {
    let store = open_store()?;
    let removed = pairing::panic_unpair(&store)
        .await
        .context("failed to revoke paired users")?;

    println!("Removed {removed} pairing/paired entries.");
    if removed > 0 {
        println!("All users must re-pair to regain access.");
    } else {
        println!("No paired users were found.");
    }
    Ok(())
}

fn open_store() -> Result<SqliteBackend> {
    let db_path = springtale_store::paths::default_db_path();
    if !db_path.exists() {
        anyhow::bail!(
            "Database not found at {}. Run `springtale init` first.",
            db_path.display()
        );
    }

    let passphrase = rpassword::read_password_from_tty(Some("Vault passphrase: "))
        .context("failed to read passphrase")?;
    let key_hex = derive_db_encryption_key_hex(passphrase.as_bytes());

    SqliteBackend::open_encrypted(&db_path, &key_hex)
        .context("failed to open encrypted database (wrong passphrase?)")
}
