//! Step 7 — distribute the gossip snapshot into each member's
//! `LocalAwareness` (`COOPERATION.md §8`).
//!
//! Per the §7 capability table, neighbor awareness is unlocked at Warming
//! tier and above. Cold-tier formations skip this step so members see only
//! their own state and the formation intent.
//!
//! Each operational member publishes its current state to the gossip
//! substrate; then the merged snapshot is read back and distributed into
//! every member's awareness (filtering out self). At Warming+ also record
//! neighbor `TickReport`s for the chain-dependency path (§7).

use std::collections::HashMap;

use crate::cooperation::formation::Formation;
use springtale_cooperation::awareness::GossipEntry;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::momentum::MomentumTier;
use springtale_cooperation::tick_processor::FormationTickResult;

pub async fn run(formation: &mut Formation, result: &FormationTickResult) {
    if formation.momentum.tier == MomentumTier::Cold {
        return;
    }

    // Publish each operational member's current state to the gossip
    // substrate. Last entry per member wins so the merged snapshot reflects
    // the most recent tick.
    let success_map: HashMap<AgentId, bool> = result
        .reports
        .iter()
        .map(|r| (r.agent_id, r.intent_alignment > 0.5))
        .collect();

    for m in formation.members.iter().filter(|m| m.is_operational()) {
        let fuel_pct = if formation.fuel.initial() > 0 {
            formation.fuel.remaining() as f32 / formation.fuel.initial() as f32
        } else {
            1.0
        };
        let entry = GossipEntry::from_state(
            m.agent_id,
            &m.health,
            m.role.name(),
            fuel_pct,
            success_map.get(&m.agent_id).copied().unwrap_or(true),
            formation.attention_broker.current().load(&m.agent_id),
        );
        formation.gossip_store.publish(entry).await;
    }

    // Read the merged snapshot and distribute into each member's
    // LocalAwareness, filtering out self.
    let snapshots = formation.gossip_store.snapshots().await;
    let tier = formation.momentum.tier;
    for m in formation.members.iter_mut() {
        for snap in &snapshots {
            if snap.agent_id != m.agent_id {
                m.awareness.update_neighbor(snap.clone());
            }
        }
        m.awareness.formation_momentum = tier;
        let neighbor_reports: Vec<springtale_cooperation::cadence::TickReport> = result
            .reports
            .iter()
            .filter(|r| r.agent_id != m.agent_id)
            .cloned()
            .collect();
        m.awareness.record_tick_reports(neighbor_reports);
    }
}
