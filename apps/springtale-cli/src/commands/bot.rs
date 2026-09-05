//! `springtale bot` subcommands — pairing management from the daemon host.
//!
//! These commands run on the trusted device (the server), never via chat.
//! They open the encrypted database directly so they work without the
//! daemon running — critical for the `panic-unpair` IPV scenario.

use anyhow::{Context, Result};

use crate::store::PassphraseOpts;
use springtale_runtime::operations::pairing;

pub async fn pair_init(opts: &PassphraseOpts) -> Result<()> {
    let store = crate::store::open_store(opts)?;
    let code = pairing::generate_pairing_code(&store)
        .await
        .context("failed to generate pairing code")?;

    println!("Pairing code (give this to the user, do NOT send via chat):\n");
    println!("  {code}\n");
    println!("The user types this code into their chat with the bot.");
    println!("Code expires in 10 minutes. Single-use.");
    Ok(())
}

pub async fn panic_unpair(opts: &PassphraseOpts) -> Result<()> {
    let store = crate::store::open_store(opts)?;
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
