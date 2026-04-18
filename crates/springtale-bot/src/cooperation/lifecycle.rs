//! Formation lifecycle — materialize and dissolve live formations.
//!
//! Called by the bot event loop when it receives FormationCommands
//! from runtime operations. This is the ONLY code that creates
//! live Formation structs from database rows.

use std::sync::Arc;

use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::types::FormationConstraints;
use springtale_store::StorageBackend;

use crate::cooperation::formation::{Formation, FormationMember};
use crate::error::BotError;

/// Materialize a Formation struct from database rows.
///
/// Reads FormationRow + FormationMemberRows from storage, looks up
/// connector capabilities from the registry, and builds a live
/// Formation ready for tick processing.
pub async fn spawn_formation(
    formation_id: &str,
    store: &Arc<dyn StorageBackend>,
    registry: &Arc<RwLock<ConnectorRegistry>>,
) -> Result<Formation, BotError> {
    // Read formation from database
    let row = store
        .get_formation(formation_id)
        .await?
        .ok_or_else(|| BotError::Handler(format!("formation not found: {formation_id}")))?;

    let member_rows = store
        .list_formation_members(formation_id)
        .await?;

    // Build members from database rows.
    // Each member gets the connector name as a capability — the connector's
    // action list is resolved at dispatch time by the orchestrator, not here.
    let reg = registry.read().await;
    let members: Vec<FormationMember> = member_rows
        .iter()
        .map(|mr| {
            let mut caps: Vec<CapabilityDecl> = vec![CapabilityDecl::new(mr.connector_name.clone())];
            // Add action names from registry if connector is loaded
            if let Some(entry) = reg.get(&mr.connector_name) {
                for action in entry.host.actions() {
                    caps.push(CapabilityDecl::with_connector(
                        action.name.clone(),
                        mr.connector_name.clone(),
                    ));
                }
            }
            FormationMember::new(AgentId::new(), caps)
        })
        .collect();
    drop(reg);

    // Parse intent from stored string
    let intent = springtale_cooperation::command::parse_intent(&row.intent);

    let mut formation = Formation::new(
        members,
        intent,
        FormationConstraints::default(),
    );

    // Override the auto-generated ID with the stored one
    if let Ok(uuid) = uuid::Uuid::parse_str(&row.id) {
        formation.id = springtale_cooperation::types::FormationId(uuid);
    }

    // Restore momentum state from DB (survives restarts)
    if let Ok(Some(momentum_row)) = store.get_formation_momentum(&row.id).await {
        formation.momentum.tier =
            springtale_cooperation::momentum::MomentumTier::parse(&momentum_row.tier);
        formation.momentum.consecutive_successes = momentum_row.consecutive_successes as u32;
        formation.momentum.interference_count = momentum_row.interference_count as u32;
    }

    // Restore rally state from DB (survives restarts)
    if let Ok(Some(rally_row)) = store.get_formation_rally(&row.id).await {
        formation.rally.tokens_remaining = rally_row.tokens_remaining as u32;
        formation.rally.max_tokens = rally_row.max_tokens as u32;
    }

    Ok(formation)
}
