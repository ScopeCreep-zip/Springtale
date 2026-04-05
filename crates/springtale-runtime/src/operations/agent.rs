//! Agent operations — autonomy level management.
//!
//! Stores autonomy levels using the bot alias table as a key-value store.
//! Key: `"autonomy:{agent_name}"`, Value: the autonomy level string.

use springtale_store::StorageBackend;

use crate::error::OperationError;

/// Valid autonomy levels (ARCHITECTURE.md §5.3).
const VALID_LEVELS: &[&str] = &[
    "observe",           // L0
    "suggest",           // L1
    "act-with-approval", // L2
    "act-autonomously",  // L3
];

/// Set the autonomy level for an agent.
///
/// Valid levels: "observe" (L0), "suggest" (L1), "act-with-approval" (L2),
/// "act-autonomously" (L3).
pub async fn set_autonomy(
    store: &dyn StorageBackend,
    agent_name: &str,
    level: &str,
) -> Result<(), OperationError> {
    if !VALID_LEVELS.contains(&level) {
        return Err(OperationError::Validation(format!(
            "invalid autonomy level '{level}': must be one of: {}",
            VALID_LEVELS.join(", ")
        )));
    }

    let alias_key = format!("autonomy:{agent_name}");
    store
        .upsert_alias(&alias_key, level, "cli")
        .await
        .map_err(OperationError::Store)?;
    Ok(())
}

/// Get the current autonomy level for an agent.
///
/// Returns "suggest" (L1) if no level has been set.
pub async fn get_autonomy(
    store: &dyn StorageBackend,
    agent_name: &str,
) -> Result<String, OperationError> {
    let alias_key = format!("autonomy:{agent_name}");
    let aliases = store.list_aliases().await.map_err(OperationError::Store)?;
    let level = aliases
        .iter()
        .find(|(k, _)| k == &alias_key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "suggest".to_owned());
    Ok(level)
}
