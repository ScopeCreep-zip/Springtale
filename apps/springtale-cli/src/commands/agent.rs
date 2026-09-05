use anyhow::Result;
use springtale_store::StorageBackend;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::AgentAction;

/// Handle agent subcommands.
pub async fn run(action: AgentAction, store: &SqliteBackend) -> Result<()> {
    match action {
        AgentAction::SetAutonomy { name, level } => {
            let target = resolve_agent_target(store, &name).await?;
            springtale_runtime::operations::agent::set_autonomy(store, &target, &level)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Agent '{name}' autonomy set to: {level}");
        }
    }
    Ok(())
}

/// Resolve a rule name or id to the rule-id-keyed autonomy target. The CLI
/// has no engine, so names are looked up in the stored rule set.
async fn resolve_agent_target(
    store: &SqliteBackend,
    name_or_id: &str,
) -> Result<springtale_runtime::operations::agent::AutonomyTarget> {
    if let Ok(rule_id) = uuid::Uuid::parse_str(name_or_id) {
        return Ok(springtale_runtime::operations::agent::AutonomyTarget::Agent { rule_id });
    }
    let rule_id = store
        .list_rules()
        .await?
        .into_iter()
        .find(|r| r.name == name_or_id)
        .map(|r| r.id.0)
        .ok_or_else(|| anyhow::anyhow!("rule '{name_or_id}' not found"))?;
    Ok(springtale_runtime::operations::agent::AutonomyTarget::Agent { rule_id })
}
