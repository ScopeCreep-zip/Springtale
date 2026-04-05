use anyhow::Result;

use springtale_store::backend::trait_::StorageBackend;

/// Execute emergency data destruction.
///
/// NO confirmation prompt — this is an emergency command for IPV survivors
/// and activists who need to destroy data immediately. The user configured
/// this command knowing what it does.
///
/// Delegates to `springtale_runtime::operations::safety::panic_wipe` which:
/// 1. Overwrites vault file with random bytes, then deletes
/// 2. Overwrites SQLite database + WAL + SHM with random bytes, then deletes
/// 3. Overwrites config file (may contain connector settings that reveal activity)
pub async fn run(store: &dyn StorageBackend) -> Result<()> {
    springtale_runtime::operations::safety::panic_wipe(store)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    eprintln!("All data destroyed.");

    Ok(())
}
