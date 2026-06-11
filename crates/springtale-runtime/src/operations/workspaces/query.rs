//! Read-side + manual-management operations for the
//! external-workspace directory.
//!
//! These are the entry points the Tauri commands + recipe deploy
//! form's WorkspaceTargetPicker dropdown call. Side-effecting
//! ones (scan, upsert, delete) flow through the store; the harvester
//! (in `harvester.rs`) handles the passive-cache path independently.

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;

use springtale_connector::factory::FactoryEntry;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::mental_model::WorkspaceProvenance;
use springtale_store::backend::StorageBackend;
use springtale_store::schema::mental_model::MentalModelWorkspaceRow;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// IPC-shaped projection of [`MentalModelWorkspaceRow`]. Flat,
/// derives `specta::Type` for the Tauri boundary.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WorkspaceInfo {
    pub workspace_key: String,
    pub connector_name: String,
    pub display_name: String,
    pub kind: String,
    /// Metadata serialized as a JSON string so specta doesn't
    /// recurse into `serde_json::Value` (per
    /// `feedback_specta_recursive_types`).
    pub metadata_json: Option<String>,
    pub first_seen_at_unix_ms: i64,
    pub last_seen_at_unix_ms: i64,
    /// Serialized `WorkspaceProvenance` enum. The frontend parses
    /// for the tooltip "discovered by … via …" copy.
    pub provenance_json: String,
}

impl From<MentalModelWorkspaceRow> for WorkspaceInfo {
    fn from(r: MentalModelWorkspaceRow) -> Self {
        Self {
            workspace_key: r.workspace_key,
            connector_name: r.connector_name,
            display_name: r.display_name,
            kind: r.kind,
            metadata_json: r.metadata_json,
            first_seen_at_unix_ms: r.first_seen_at_unix_ms,
            last_seen_at_unix_ms: r.last_seen_at_unix_ms,
            provenance_json: r.provenance_json,
        }
    }
}

/// List every workspace in a formation, optionally filtered by
/// connector. Newest-first per the index on `last_seen_at DESC`.
pub async fn list_workspaces(
    store: &Arc<dyn StorageBackend>,
    formation_id: &str,
    connector_filter: Option<&str>,
) -> Result<Vec<WorkspaceInfo>, OperationError> {
    let rows = store
        .mental_model_workspaces_for_formation(formation_id, connector_filter)
        .await
        .map_err(OperationError::Store)?;
    Ok(rows.into_iter().map(WorkspaceInfo::from).collect())
}

/// Active discovery — dispatch the connector's
/// `discover_destinations` action, upsert each result into the
/// formation's directory with `WorkspaceProvenance::ActiveDiscovery`,
/// return the updated list.
///
/// Connectors that don't ship a `discover_destinations` action
/// return an empty result list; the harvester handles their
/// passive-cache path. Telegram is the canonical example.
pub async fn scan_workspaces(
    state: &RuntimeState,
    formation_id: &str,
    connector_name: &str,
) -> Result<Vec<WorkspaceInfo>, OperationError> {
    let momentum = springtale_cooperation::momentum::MomentumTier::Warming;
    let tier = crate::cooperation::momentum_to_wasm_tier(momentum);
    let exec = state
        .capability_bridge
        .execute(
            connector_name,
            "discover_destinations",
            serde_json::json!({}),
            tier,
        )
        .await;
    let now_ms = Utc::now().timestamp_millis();
    let provenance = WorkspaceProvenance::ActiveDiscovery {
        scanned_at: Utc::now(),
    };
    let provenance_json = serde_json::to_string(&provenance)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;
    match exec {
        Ok(result) => {
            // Action contract: output is `{ workspaces: [...] }`
            // where each row matches the shape of
            // `springtale_connector::mention::HarvestedDestination`.
            let destinations = result
                .output
                .get("workspaces")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for dest in destinations {
                let Some(workspace_key) = dest
                    .get("workspace_key")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                else {
                    continue;
                };
                let display_name = dest
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&workspace_key)
                    .to_owned();
                let kind = dest
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_owned();
                let metadata = dest
                    .get("metadata")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let metadata_json = if metadata.is_null()
                    || metadata.as_object().map(|o| o.is_empty()).unwrap_or(false)
                {
                    None
                } else {
                    Some(
                        serde_json::to_string(&metadata)
                            .map_err(|e| OperationError::Serialization(e.to_string()))?,
                    )
                };
                let row = MentalModelWorkspaceRow {
                    workspace_key,
                    connector_name: connector_name.to_owned(),
                    display_name,
                    kind,
                    metadata_json,
                    first_seen_at_unix_ms: now_ms,
                    last_seen_at_unix_ms: now_ms,
                    provenance_json: provenance_json.clone(),
                };
                state
                    .store
                    .mental_model_workspace_upsert(formation_id, &row)
                    .await
                    .map_err(OperationError::Store)?;
            }
        }
        Err(e) => {
            // Connector doesn't implement discover_destinations
            // (Telegram), or the call failed (network, auth).
            // Either way, fall through — the dropdown will still
            // render whatever's already in the directory.
            tracing::info!(
                connector = %connector_name,
                error = %e,
                "discover_destinations unavailable — relying on passive harvest"
            );
        }
    }
    list_workspaces(&state.store, formation_id, Some(connector_name)).await
}

/// Drop one workspace from the formation's directory. The
/// harvester won't recreate it until the connector emits another
/// event mentioning the key.
pub async fn delete_workspace(
    store: &Arc<dyn StorageBackend>,
    formation_id: &str,
    workspace_key: &str,
) -> Result<(), OperationError> {
    store
        .mental_model_workspace_delete(formation_id, workspace_key)
        .await
        .map_err(OperationError::Store)
}

/// Pre-deploy onboarding URL resolver — connector-agnostic.
///
/// The recipe deploy form has the user's connector config (bot token,
/// API base, etc.) but the connector is NOT yet registered with the
/// runtime — registration happens after Deploy. To resolve the
/// onboarding deep link before then, we walk the inventory of
/// [`FactoryEntry`]s, find the factory matching `connector_name`,
/// build a one-shot instance from the supplied config, dispatch the
/// connector's `onboard_url` action, and discard the instance.
///
/// Telegram returns `https://t.me/<bot>?start=…`. Other connectors
/// that implement an `onboard_url` action (e.g. Discord OAuth invite
/// URLs, Slack app install URLs) plug into this same path without
/// any changes here — the entire flow is the connector trait
/// boundary, exactly as `connector-guidelines.md` requires.
///
/// Connectors that have no `onboard_url` action surface a clean
/// `unknown action` error which the frontend can render as "no
/// onboarding flow for this connector".
pub async fn preview_onboard_url(
    connector_name: &str,
    config: serde_json::Value,
    payload: &str,
) -> Result<String, OperationError> {
    let factory = inventory::iter::<FactoryEntry>
        .into_iter()
        .find(|e| e.factory.name() == connector_name)
        .ok_or_else(|| {
            OperationError::Connector(format!("no factory registered for {connector_name}"))
        })?
        .factory;

    let connector = factory
        .create(config)
        .await
        .map_err(|e| OperationError::Connector(format!("{connector_name} factory.create: {e}")))?;

    let input = serde_json::json!({ "payload": payload });
    let result = connector
        .execute("onboard_url", input)
        .await
        .map_err(|e| OperationError::Connector(format!("{connector_name} onboard_url: {e}")))?;

    let url = result
        .output
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            OperationError::Connector(format!(
                "{connector_name} onboard_url returned no `url` field"
            ))
        })?
        .to_owned();
    Ok(url)
}

/// Manually register a workspace the user typed into the recipe
/// form's manual-entry escape hatch. Provenance is
/// [`WorkspaceProvenance::ManualEntry`].
pub async fn upsert_workspace_manual(
    store: &Arc<dyn StorageBackend>,
    formation_id: &str,
    entered_by: AgentId,
    workspace_key: String,
    display_name: String,
    connector_name: String,
    kind: String,
) -> Result<(), OperationError> {
    let now_ms = Utc::now().timestamp_millis();
    let provenance = WorkspaceProvenance::ManualEntry { entered_by };
    let provenance_json = serde_json::to_string(&provenance)
        .map_err(|e| OperationError::Serialization(e.to_string()))?;
    let row = MentalModelWorkspaceRow {
        workspace_key,
        connector_name,
        display_name,
        kind,
        metadata_json: None,
        first_seen_at_unix_ms: now_ms,
        last_seen_at_unix_ms: now_ms,
        provenance_json,
    };
    store
        .mental_model_workspace_upsert(formation_id, &row)
        .await
        .map_err(OperationError::Store)
}

// The harvester (used by the bot's event loop, not by Tauri) is
// exposed in `harvester.rs`. It does NOT need `RuntimeState` — it
// only needs the store + registry handles, which the bot already
// has.
