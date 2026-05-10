//! Formation lifecycle — materialize and dissolve live formations.
//!
//! Called by the bot event loop when it receives FormationCommands
//! from runtime operations. This is the ONLY code that creates
//! live Formation structs from database rows.

use std::sync::Arc;

use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::awareness::GossipStore;
use springtale_cooperation::cadence::{AgentId, CadenceBus};
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::handoff::FlexibleChainPool;
use springtale_cooperation::types::FormationConstraints;
use springtale_store::StorageBackend;

use crate::cooperation::formation::{Formation, FormationDeps, FormationMember};
use crate::error::BotError;

/// Materialize a Formation struct from database rows.
///
/// Reads FormationRow + FormationMemberRows from storage, looks up
/// connector capabilities from the registry, and builds a live
/// Formation ready for tick processing.
///
/// The `cadence` and `gossip_store` arguments come from the runtime
/// (one instance shared across every formation the daemon spawns).
/// The `flex_chain_pool` is per-formation — each formation has its own
/// crossbeam-deque pool scoped to its own members' capabilities.
pub async fn spawn_formation(
    formation_id: &str,
    store: &Arc<dyn StorageBackend>,
    registry: &Arc<RwLock<ConnectorRegistry>>,
    cadence: &Arc<CadenceBus>,
    gossip_store: &Arc<dyn GossipStore>,
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

    let deps = FormationDeps {
        cadence: cadence.clone(),
        store: store.clone(),
        gossip_store: gossip_store.clone(),
        flex_chain_pool: Arc::new(FlexibleChainPool::new()),
    };
    let (mut formation, proto_dispatch, ack_dispatch) = Formation::new(
        members,
        intent,
        FormationConstraints::default(),
        deps,
    );

    // Override the auto-generated ID with the stored one
    if let Ok(uuid) = uuid::Uuid::parse_str(&row.id) {
        formation.id = springtale_cooperation::types::FormationId(uuid);
    }

    // Spawn the bus dispatcher tasks. Protocol dispatcher fans out
    // ProtocolMsg by MessageTarget; ack consumer drains IntentAckMsg
    // to (currently) a lightweight log — cadence integration of the
    // ack consumer happens via Formation's cadence evaluator in Phase
    // 18. For now the ack handler records the interpretation at debug
    // level so the mpsc router doesn't fill.
    let formation_id = formation.id;
    let proto_handle = tokio::spawn(
        springtale_cooperation::comms::dispatcher::protocol::run(
            proto_dispatch,
            // Nearest-capable resolver: currently unused (no routing data
            // plumbed through lifecycle yet). Formation-level capability
            // index arrives with §20 flex_chain integration; for now we
            // return None so NearestCapable messages are dropped, and the
            // spec-canonical Specific+Formation targets always route.
            |_cap| None,
        ),
    );
    let ack_handle = tokio::spawn(
        springtale_cooperation::comms::dispatcher::ack::run(ack_dispatch, move |ack| {
            tracing::debug!(
                formation_id = %formation_id.0,
                source = ?ack.source,
                interpretation = %ack.interpretation,
                "intent acknowledged"
            );
        }),
    );
    formation.protocol_dispatcher = Some(proto_handle);
    formation.ack_dispatcher = Some(ack_handle);

    // Spawn one `member_runner` task per member so each agent can respond
    // to L4 CFPs (B2). Runners are aborted on dissolve via `Formation::Drop`
    // and on individual leaves via `Formation::leave`.
    formation.start_member_runners();

    // Restore momentum state from DB (survives restarts)
    if let Ok(Some(momentum_row)) = store.get_formation_momentum(&row.id).await {
        formation.momentum.tier =
            springtale_cooperation::momentum::MomentumTier::parse(&momentum_row.tier);
        formation.momentum.consecutive_successes = momentum_row.consecutive_successes as u32;
        formation.momentum.interference_count = momentum_row.interference_count as u32;
    }

    // Restore rally state from DB (survives restarts). `FormationRally`
    // constructs with a fresh `Semaphore(max)`; we align its remaining
    // permit count to disk state by consuming `(max - remaining)` up
    // front. See `FormationRally::restore_tokens` for the exact logic.
    if let Ok(Some(rally_row)) = store.get_formation_rally(&row.id).await {
        formation
            .rally
            .restore_tokens(rally_row.tokens_remaining as usize);
    }

    // Restore the shared mental model (§21) from persistence. A fresh
    // formation or one with no prior history gets `SharedMentalModel::default()`;
    // an existing one recovers its accumulated domain knowledge,
    // capability awareness, cooperation patterns, vocabulary, and
    // conventions so the recovered formation behaves as if it hadn't
    // restarted.
    let mm_store = springtale_cooperation::mental_model::BackendStore::new(store.clone());
    match springtale_cooperation::mental_model::Store::load(
        &mm_store,
        &formation.id.0.to_string(),
    )
    .await
    {
        Ok(model) => formation.mental_model = model,
        Err(e) => tracing::warn!(
            formation_id = %formation.id.0,
            error = %e,
            "failed to load mental model from store; starting empty"
        ),
    }

    Ok(formation)
}

/// Persist a formation's mental-model state before it's dropped. Per
/// spec §21 the model accumulates over time — without this save pass,
/// every dissolve resets the formation's accumulated convention /
/// pattern / vocabulary knowledge. Called from the dissolve path in
/// `event_loop::handle_formation_command`.
pub async fn persist_mental_model(
    formation: &crate::cooperation::formation::Formation,
    store: &Arc<dyn StorageBackend>,
) -> Result<(), BotError> {
    let mm_store =
        springtale_cooperation::mental_model::BackendStore::new(store.clone());
    springtale_cooperation::mental_model::Store::save(
        &mm_store,
        &formation.id.0.to_string(),
        &formation.mental_model,
    )
    .await
    .map_err(|e| BotError::Handler(format!("persist mental model: {e}")))?;
    Ok(())
}
