//! Awareness system — local neighbor perception.
//!
//! Per COOPERATION.pdf §8: "Total War morale as composite local signal.
//! Each unit's morale is continuously modified by: casualties taken,
//! casualties inflicted, flank/rear attacks, nearby friendly routing,
//! general proximity, fatigue, experience, charge state."
//!
//! Agents don't have global visibility. They perceive neighbors through
//! local awareness — what nearby agents are doing, their health, their
//! role, and their recent tick reports. Decisions are made locally
//! based on this partial information.
//!
//! Available at Warming+ tier (§7 capability table).

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cadence::{AgentId, TickReport};
use crate::supervision::Liveness;
use crate::types::AgentHealth;
use crate::momentum::MomentumTier;

/// Role identity as surfaced across gossip and the canvas UI.
///
/// Per COOPERATION.md §14 we deliberately keep `Box<dyn DynamicRoleTrait>`
/// local (no typetag serde — community roles ship as WASM and can't be
/// registered into the host's `inventory` crate). The serializable
/// `RoleSignature` is what crosses gossip and persistence boundaries —
/// a value-type enum that names the role by kind.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Type)]
pub enum RoleSignature {
    General,
    Information,
    Support,
    /// Community WASM-delivered role name. First-party roles never use
    /// this variant — they use their named variant above.
    Custom(String),
}

impl RoleSignature {
    /// Parse a role name string into a RoleSignature. Unknown names
    /// become `Custom(name)` — this is how community/WASM roles enter
    /// the type system without needing host-side registration.
    pub fn parse(name: &str) -> Self {
        match name {
            "General" => Self::General,
            "Information" => Self::Information,
            "Support" => Self::Support,
            other => Self::Custom(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::General => "General",
            Self::Information => "Information",
            Self::Support => "Support",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for RoleSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for RoleSignature {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&str> for RoleSignature {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

/// Snapshot of a neighboring agent's observable state.
///
/// Per Splinter Cell's asymmetric information fusion: each agent
/// sees different things. The high player sees through windows;
/// the ground player hears conversations. Neither has complete
/// information.
#[derive(Debug, Clone)]
pub struct NeighborSnapshot {
    pub agent_id: AgentId,
    pub health: AgentHealth,
    /// Canonical role identity. Serializable (unlike
    /// `Box<dyn DynamicRoleTrait>`), so it crosses gossip/persistence
    /// boundaries cleanly. See [`RoleSignature`].
    pub role: RoleSignature,
    pub fuel_remaining_pct: f32,
    pub last_action_success: bool,
    pub attention_load: f32,
    pub liveness: Liveness,
    pub last_updated: Instant,
}

/// What an agent knows about its local environment.
///
/// Per Total War: morale is a composite local signal — not a
/// global metric. Each unit computes its own morale from what
/// it can see nearby. This creates emergent behavior: units
/// near a routing ally take morale penalties, units near a
/// general get rally bonuses.
#[derive(Debug, Clone)]
pub struct LocalAwareness {
    /// Snapshots of neighboring agents' states.
    pub neighbor_states: HashMap<AgentId, NeighborSnapshot>,
    /// Current momentum tier of the formation (shared knowledge).
    pub formation_momentum: MomentumTier,
    /// Recent tick reports from neighbors (Warming+ only).
    pub last_tick_reports: Vec<TickReport>,
}

impl Default for LocalAwareness {
    fn default() -> Self {
        Self {
            neighbor_states: HashMap::new(),
            formation_momentum: MomentumTier::Cold,
            last_tick_reports: Vec::new(),
        }
    }
}

impl LocalAwareness {
    /// Update a neighbor's snapshot.
    pub fn update_neighbor(&mut self, snapshot: NeighborSnapshot) {
        self.neighbor_states.insert(snapshot.agent_id, snapshot);
    }

    /// Remove a neighbor (disconnected or dead).
    pub fn remove_neighbor(&mut self, agent_id: &AgentId) {
        self.neighbor_states.remove(agent_id);
    }

    /// Get count of healthy neighbors.
    pub fn healthy_neighbor_count(&self) -> usize {
        self.neighbor_states
            .values()
            .filter(|n| {
                matches!(
                    n.health,
                    AgentHealth::Operational | AgentHealth::Degraded { .. }
                )
            })
            .count()
    }

    /// Get count of neighbors in distress (incapacitated or dead).
    pub fn distressed_neighbor_count(&self) -> usize {
        self.neighbor_states
            .values()
            .filter(|n| {
                matches!(
                    n.health,
                    AgentHealth::Incapacitated | AgentHealth::Dead { .. }
                )
            })
            .count()
    }

    /// Compute a local "morale" score (0.0-1.0) from neighbor states.
    ///
    /// Per Total War: morale drops when nearby allies are routing,
    /// flanked, or taking heavy casualties. It rises when the general
    /// is nearby and allies are winning.
    pub fn local_morale(&self) -> f32 {
        if self.neighbor_states.is_empty() {
            return 0.5; // neutral when alone
        }

        let total = self.neighbor_states.len() as f32;
        let healthy = self.healthy_neighbor_count() as f32;
        let distressed = self.distressed_neighbor_count() as f32;

        // Base morale from neighbor health ratio
        let health_factor = healthy / total;

        // Penalty for distressed neighbors (cascade risk)
        let distress_penalty = distressed / total * 0.3;

        // Momentum bonus
        let momentum_bonus = match self.formation_momentum {
            MomentumTier::Cold => 0.0,
            MomentumTier::Warming => 0.05,
            MomentumTier::Hot => 0.1,
            MomentumTier::Fever => 0.2,
        };

        (health_factor - distress_penalty + momentum_bonus).clamp(0.0, 1.0)
    }

    /// Record tick reports from neighbors (for Warming+ awareness).
    pub fn record_tick_reports(&mut self, reports: Vec<TickReport>) {
        // Keep only the most recent reports (prevent unbounded growth)
        self.last_tick_reports = reports;
    }

    /// Check if any neighbor recently interfered with this agent.
    pub fn has_recent_interference(&self, my_id: &AgentId) -> bool {
        self.last_tick_reports
            .iter()
            .any(|r| r.interference_with.contains(my_id))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_snapshot(id: AgentId, health: AgentHealth) -> NeighborSnapshot {
        NeighborSnapshot {
            agent_id: id,
            health,
            role: RoleSignature::General,
            fuel_remaining_pct: 1.0,
            last_action_success: true,
            attention_load: 0.5,
            liveness: Liveness::Alive,
            last_updated: Instant::now(),
        }
    }

    #[test]
    fn test_empty_awareness() {
        let awareness = LocalAwareness::default();
        assert_eq!(awareness.healthy_neighbor_count(), 0);
        assert_eq!(awareness.distressed_neighbor_count(), 0);
        assert!((awareness.local_morale() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_healthy_neighbors() {
        let mut awareness = LocalAwareness::default();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        awareness.update_neighbor(make_snapshot(id1, AgentHealth::Operational));
        awareness.update_neighbor(make_snapshot(id2, AgentHealth::Operational));
        assert_eq!(awareness.healthy_neighbor_count(), 2);
        assert!(awareness.local_morale() > 0.9);
    }

    #[test]
    fn test_distressed_neighbors_lower_morale() {
        let mut awareness = LocalAwareness::default();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        awareness.update_neighbor(make_snapshot(id1, AgentHealth::Operational));
        awareness.update_neighbor(make_snapshot(id2, AgentHealth::Incapacitated));
        assert!(awareness.local_morale() < 0.7);
        assert_eq!(awareness.distressed_neighbor_count(), 1);
    }

    #[test]
    fn test_momentum_boosts_morale() {
        let mut awareness = LocalAwareness::default();
        // Mix of healthy + distressed so morale isn't clamped at 1.0
        awareness.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Operational));
        awareness.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Incapacitated));

        awareness.formation_momentum = MomentumTier::Cold;
        let cold_morale = awareness.local_morale();

        awareness.formation_momentum = MomentumTier::Fever;
        let fever_morale = awareness.local_morale();

        assert!(fever_morale > cold_morale);
    }

    #[test]
    fn test_remove_neighbor() {
        let mut awareness = LocalAwareness::default();
        let id = AgentId::new();
        awareness.update_neighbor(make_snapshot(id, AgentHealth::Operational));
        assert_eq!(awareness.healthy_neighbor_count(), 1);

        awareness.remove_neighbor(&id);
        assert_eq!(awareness.healthy_neighbor_count(), 0);
    }

    #[test]
    fn test_interference_detection() {
        let mut awareness = LocalAwareness::default();
        let my_id = AgentId::new();
        let other_id = AgentId::new();

        let report = TickReport {
            agent_id: other_id,
            tick_sequence: 1,
            action_taken: Some(crate::cadence::ActionDescriptor {
                kind: "write".to_owned(),
                target: None,
                payload_hash: 0,
            }),
            latency: std::time::Duration::from_millis(10),
            intent_alignment: 0.8,
            interference_with: vec![my_id],
        };

        awareness.record_tick_reports(vec![report]);
        assert!(awareness.has_recent_interference(&my_id));
        assert!(!awareness.has_recent_interference(&other_id));
    }
}
