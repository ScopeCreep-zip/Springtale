//! Universal mention harvester (D1).
//!
//! Runs in the bot-runtime event-loop on every dispatched
//! [`TriggerEvent`]: calls the firing connector's
//! [`MentionExtractor`] and upserts each harvested destination
//! into every formation that has the connector as a member.
//!
//! ## Scope
//!
//! "Every formation that has the connector as a member" matches
//! the cooperation framework's formation-scoping semantics — a
//! formation's `SharedMentalModel` should contain only destinations
//! reachable through one of its members. Different formations
//! have different directories.
//!
//! When no formation includes the firing connector (e.g. solo
//! pre-deploy), the harvest is a no-op. The user's deploy flow
//! creates the formation; subsequent events harvest correctly.
//!
//! ## Privacy
//!
//! The harvester writes only sizes / display labels — never
//! message bodies, never roster lists past a count. Matches the
//! executions-log privacy posture.
//!
//! ## Cooperation alignment
//!
//! Each upsert applies the gossip-delta merge
//! ([`springtale_cooperation::mental_model::merge_gossip_delta`])
//! when the directory already has an entry for the workspace key.
//! Local-origin entries always win on first observation; gossip
//! replication takes over when peers learn about the same
//! destination.

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::RwLock;

use springtale_connector::mention::HarvestedDestination;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::mental_model::WorkspaceProvenance;
use springtale_store::backend::StorageBackend;
use springtale_store::schema::mental_model::MentalModelWorkspaceRow;

use crate::error::OperationError;

/// Drive the harvester over one dispatched event.
///
/// `connector_name` is the connector that emitted the event;
/// `trigger` is the trigger name (e.g. `"command_received"`);
/// `payload` is the event's JSON body. The `agent_id` records
/// which agent observed the harvest — used as the `entered_by`
/// in the manual-entry-like provenance when this becomes a
/// passive harvest event.
///
/// Returns the number of (formation, workspace_key) rows
/// upserted. Zero means either no destinations were extracted or
/// no formation contains this connector — both are valid
/// outcomes.
pub async fn harvest_event(
    store: &Arc<dyn StorageBackend>,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    connector_name: &str,
    trigger: &str,
    payload: &Value,
    agent_id: Option<AgentId>,
) -> Result<u64, OperationError> {
    // Step 1 — pull destinations from the connector's extractor.
    // The registry exposes `Arc<dyn ConnectorHost>`; the host
    // delegates to the inner connector's `mention_extractor()`.
    // Connectors without an extractor (cron, filesystem, http,
    // browser, shell) return `None` and the harvest is a no-op.
    let destinations: Vec<HarvestedDestination> = {
        let reg = registry.read().await;
        let entry = match reg.get(connector_name) {
            Some(e) => e,
            None => return Ok(0),
        };
        match entry.host.mention_extractor() {
            Some(ext) => ext.extract(trigger, payload),
            None => return Ok(0),
        }
    };
    if destinations.is_empty() {
        return Ok(0);
    }

    // Step 2 — which formations are interested?
    // A formation is "interested" when one of its members is
    // this connector. We over-fetch via list_formations() because
    // there's no native "formations containing connector" query
    // and the per-install formation count is small (< 100 even
    // in heavy use).
    let formations = store
        .list_formations()
        .await
        .map_err(OperationError::Store)?;
    if formations.is_empty() {
        return Ok(0);
    }

    let now_ms = Utc::now().timestamp_millis();
    let provenance = WorkspaceProvenance::PassiveHarvest {
        trigger: format!("{connector_name}:{trigger}"),
        at: Utc::now(),
    };
    let provenance_json = serde_json::to_string(&provenance)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;

    let mut upserts: u64 = 0;
    for formation in formations {
        let members = store
            .list_formation_members(&formation.id)
            .await
            .map_err(OperationError::Store)?;
        let member_of = members.iter().any(|m| m.connector_name == connector_name);
        if !member_of {
            continue;
        }
        for destination in &destinations {
            let metadata_json = if destination.metadata.is_null()
                || destination
                    .metadata
                    .as_object()
                    .map(|o| o.is_empty())
                    .unwrap_or(false)
            {
                None
            } else {
                Some(
                    serde_json::to_string(&destination.metadata)
                        .map_err(|e| OperationError::Serialization(e.to_string()))?,
                )
            };
            let row = MentalModelWorkspaceRow {
                workspace_key: destination.workspace_key.clone(),
                connector_name: connector_name.to_owned(),
                display_name: destination.display_name.clone(),
                kind: destination.kind.clone(),
                metadata_json,
                first_seen_at_unix_ms: now_ms,
                last_seen_at_unix_ms: now_ms,
                provenance_json: provenance_json.clone(),
            };
            store
                .mental_model_workspace_upsert(&formation.id, &row)
                .await
                .map_err(OperationError::Store)?;
            upserts += 1;
        }
    }
    // `agent_id` is currently unused — kept on the signature for
    // forward-compat when the bot runtime threads its own agent
    // identity through (per-agent harvest provenance arrives in
    // the next iteration of cooperation scoping).
    let _ = agent_id;
    Ok(upserts)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn provenance_json_is_compact_and_round_trips() {
        let prov = WorkspaceProvenance::PassiveHarvest {
            trigger: "connector-telegram:command_received".into(),
            at: Utc::now(),
        };
        let s = serde_json::to_string(&prov).unwrap();
        let back: WorkspaceProvenance = serde_json::from_str(&s).unwrap();
        assert_eq!(back, prov);
    }
}
