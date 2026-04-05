use anyhow::Result;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::AgentAction;

/// Handle agent subcommands.
pub async fn run(action: AgentAction, store: &SqliteBackend) -> Result<()> {
    match action {
        AgentAction::SetAutonomy { name, level } => {
            springtale_runtime::operations::agent::set_autonomy(store, &name, &level)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Agent '{name}' autonomy set to: {level}");
        }
    }
    Ok(())
}
