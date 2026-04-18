//! Gossip protocol — in-process neighbor state exchange.
//!
//! Per COOPERATION.pdf §8: "Total War morale as composite local signal.
//! Each unit's morale is continuously modified by: casualties taken,
//! casualties inflicted, flank/rear attacks, nearby friendly routing,
//! general proximity, fatigue, experience, charge state."
//!
//! This is the in-process implementation. All members share the same
//! Rust process, so gossip is direct snapshot exchange — no network.
//! Cross-process gossip (chitchat/foca) deferred to Phase 3/Veilid.

use std::time::Instant;

use crate::cadence::{AgentId, TickReport};
use crate::momentum::MomentumTier;
use crate::types::AgentHealth;

use super::{LocalAwareness, NeighborSnapshot};

/// Data about a member needed to build neighbor snapshots.
/// Caller provides this from FormationMember fields.
pub struct MemberState<'a> {
    pub agent_id: AgentId,
    pub awareness: &'a mut LocalAwareness,
    pub health: &'a AgentHealth,
    pub role_name: String,
    pub fuel_pct: f32,
    pub attention_load: f32,
    pub last_success: bool,
}

/// Update each member's awareness with snapshots of all other members.
///
/// Called after process_tick(), uses the same TickReports. Only runs
/// at Warming+ tier (per spec: "Read neighbor reports" unlocked at Warming).
///
/// Uses existing `LocalAwareness::update_neighbor()` — no new tracking logic.
pub fn update_awareness(
    members: &mut [MemberState<'_>],
    reports: &[TickReport],
    momentum: MomentumTier,
) {
    // Per §7 capability table: read neighbors requires Warming+
    if matches!(momentum, MomentumTier::Cold) {
        return;
    }

    // Build snapshots for all members first (avoids borrow issues)
    let snapshots: Vec<NeighborSnapshot> = members
        .iter()
        .map(|m| NeighborSnapshot {
            agent_id: m.agent_id,
            health: m.health.clone(),
            role_name: m.role_name.clone(),
            fuel_remaining_pct: m.fuel_pct,
            last_action_success: m.last_success,
            attention_load: m.attention_load,
            liveness: crate::supervision::Liveness::Alive,
            last_updated: Instant::now(),
        })
        .collect();

    // Distribute snapshots: each member gets snapshots of all OTHER members
    for member in members.iter_mut() {
        for snapshot in &snapshots {
            if snapshot.agent_id != member.agent_id {
                member.awareness.update_neighbor(snapshot.clone());
            }
        }

        // Update formation momentum (shared knowledge)
        member.awareness.formation_momentum = momentum;

        // Record tick reports from neighbors (Warming+ only)
        let neighbor_reports: Vec<TickReport> = reports
            .iter()
            .filter(|r| r.agent_id != member.agent_id)
            .cloned()
            .collect();
        member.awareness.record_tick_reports(neighbor_reports);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_member(id: AgentId) -> (AgentId, LocalAwareness, AgentHealth) {
        (
            id,
            LocalAwareness::default(),
            AgentHealth::Operational,
        )
    }

    fn make_report(agent: AgentId) -> TickReport {
        TickReport {
            agent_id: agent,
            tick_sequence: 1,
            action_taken: Some(crate::cadence::ActionDescriptor {
                kind: "work".to_owned(),
                target: None,
                payload_hash: 0,
            }),
            latency: Duration::from_millis(5),
            intent_alignment: 0.9,
            interference_with: vec![],
        }
    }

    #[test]
    fn test_cold_skips_gossip() {
        let a = AgentId::new();
        let (_, mut aw_a, health_a) = make_member(a);
        let mut members = vec![MemberState {
            agent_id: a,
            awareness: &mut aw_a,
            health: &health_a,
            role_name: "General".to_owned(),
            fuel_pct: 1.0,
            attention_load: 0.5,
            last_success: true,
        }];

        update_awareness(&mut members, &[], MomentumTier::Cold);
        assert!(members[0].awareness.neighbor_states.is_empty());
    }

    #[test]
    fn test_warming_distributes_snapshots() {
        let a = AgentId::new();
        let b = AgentId::new();
        let (_, mut aw_a, health_a) = make_member(a);
        let (_, mut aw_b, health_b) = make_member(b);

        let reports = vec![make_report(a), make_report(b)];

        {
            let mut members = vec![
                MemberState {
                    agent_id: a,
                    awareness: &mut aw_a,
                    health: &health_a,
                    role_name: "General".to_owned(),
                    fuel_pct: 1.0,
                    attention_load: 0.5,
                    last_success: true,
                },
                MemberState {
                    agent_id: b,
                    awareness: &mut aw_b,
                    health: &health_b,
                    role_name: "General".to_owned(),
                    fuel_pct: 0.8,
                    attention_load: 0.3,
                    last_success: true,
                },
            ];

            update_awareness(&mut members, &reports, MomentumTier::Warming);
        }

        // Agent A should see agent B as neighbor
        assert_eq!(aw_a.neighbor_states.len(), 1);
        assert!(aw_a.neighbor_states.contains_key(&b));

        // Agent B should see agent A as neighbor
        assert_eq!(aw_b.neighbor_states.len(), 1);
        assert!(aw_b.neighbor_states.contains_key(&a));

        // Both should have neighbor tick reports (not their own)
        assert_eq!(aw_a.last_tick_reports.len(), 1);
        assert_eq!(aw_a.last_tick_reports[0].agent_id, b);
    }

    #[test]
    fn test_momentum_propagated() {
        let a = AgentId::new();
        let (_, mut aw_a, health_a) = make_member(a);
        let mut members = vec![MemberState {
            agent_id: a,
            awareness: &mut aw_a,
            health: &health_a,
            role_name: "General".to_owned(),
            fuel_pct: 1.0,
            attention_load: 0.5,
            last_success: true,
        }];

        update_awareness(&mut members, &[], MomentumTier::Hot);
        assert_eq!(members[0].awareness.formation_momentum, MomentumTier::Hot);
    }
}
