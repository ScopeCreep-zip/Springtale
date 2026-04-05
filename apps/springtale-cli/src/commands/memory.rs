use anyhow::Result;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::MemoryAction;

/// Handle memory subcommands.
pub async fn run(action: MemoryAction, store: &SqliteBackend) -> Result<()> {
    match action {
        MemoryAction::Audit => {
            let result = springtale_runtime::operations::memory::audit_memory(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{}", result.total_memory_note);
            if result.sessions.is_empty() {
                println!("No active sessions.");
            } else {
                for session in &result.sessions {
                    println!(
                        "  {} / {} (created: {})",
                        session.user_id, session.channel_id, session.created_at
                    );
                }
            }
        }
        MemoryAction::Compact { max_entries } => {
            let deleted =
                springtale_runtime::operations::memory::compact_memory(store, max_entries)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Compacted: {deleted} entries removed.");
        }
    }
    Ok(())
}
