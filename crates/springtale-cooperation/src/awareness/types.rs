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
use crate::momentum::MomentumTier;
use crate::supervision::Liveness;
use crate::types::AgentHealth;

/// §A.4 `percent_update_per_tick` — morale lerps this fraction toward its target
/// each tick (Total War WH3 default 0.15). Gradual routing, not snap changes.
pub const MORALE_LERP: f32 = 0.15;
/// §A.4 `minimium_increment_update_per_tick` (normalized to the 0–1 morale
/// scale) — the smallest non-zero step so morale always converges to its target.
pub const MORALE_MIN_STEP: f32 = 0.01;
/// §A.4 `max_routing_friends_to_consider` — at most this many distressed
/// neighbors contribute to the morale-drop contagion, bounding panic spread
/// (decisions §11 #7). The WH3 `max_routing_enemies_to_consider = 5` cap has no
/// analog in a cooperative (non-adversarial) formation, so only this cap applies.
pub const MAX_CONTAGION_DISTRESSED: usize = 4;

/// §A.4 rally falloff (decisions §11 #6), non-spatial analog. WH3's rally
/// aura is full-strength out to `general_aura_radius = 70` units, then
/// linear to zero at `70 × inspiration_radius_max_effect_range_modifier
/// (1.5) = 105`. Springtale formations have no spatial geometry; the
/// "distance" between agents is the **Age of Information** of the
/// neighbor's gossip snapshot (the AoI temporal-decay model from the
/// gossip-network literature). A snapshot fresher than `AOI_FULL_EFFECT`
/// contributes at full weight; influence falls linearly to zero at
/// `AOI_FULL_EFFECT × AOI_ZERO_FACTOR` — the same 1.0→1.5 falloff shape
/// as the WH3 aura.
pub const AOI_FULL_EFFECT: std::time::Duration = std::time::Duration::from_secs(2);
/// Falloff endpoint multiplier — mirrors WH3's
/// `inspiration_radius_max_effect_range_modifier = 1.5`.
pub const AOI_ZERO_FACTOR: f32 = 1.5;

/// Linear AoI influence weight: 1.0 while fresh (≤ [`AOI_FULL_EFFECT`]),
/// falling linearly to 0.0 at `AOI_FULL_EFFECT × AOI_ZERO_FACTOR`.
pub fn aoi_weight(age: std::time::Duration) -> f32 {
    let full = AOI_FULL_EFFECT.as_secs_f32();
    let zero = full * AOI_ZERO_FACTOR;
    let age = age.as_secs_f32();
    if age <= full {
        1.0
    } else {
        ((zero - age) / (zero - full)).clamp(0.0, 1.0)
    }
}

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
    /// Lerped Total War morale (0.0–1.0), §A.4. Advanced each tick by
    /// [`Self::tick_morale`] toward [`Self::morale_target`] at [`MORALE_LERP`];
    /// read via [`Self::local_morale`]. Stateful (not instantaneous) so routing
    /// is gradual and bounded, per decisions §11 #8.
    pub morale: f32,
}

impl Default for LocalAwareness {
    fn default() -> Self {
        Self {
            neighbor_states: HashMap::new(),
            formation_momentum: MomentumTier::Cold,
            last_tick_reports: Vec::new(),
            morale: 0.5, // neutral
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

    /// Target morale (0.0–1.0) computed from the CURRENT neighbor states — the
    /// value [`Self::local_morale`] gradually lerps toward.
    ///
    /// Per Total War: morale drops when nearby allies are routing, flanked, or
    /// taking heavy casualties; it rises with momentum. The distress contagion
    /// is bounded to [`MAX_CONTAGION_DISTRESSED`] neighbors so panic cannot
    /// spread unboundedly (decisions §11 #7).
    pub fn morale_target(&self) -> f32 {
        if self.neighbor_states.is_empty() {
            return 0.5; // neutral when alone
        }

        let total = self.neighbor_states.len() as f32;
        // §A.4 rally falloff (decisions §11 #6): every neighbor's influence
        // is weighted by the Age of Information of its snapshot — fresh
        // gossip counts fully, stale gossip fades out linearly (the WH3
        // 70/×1.5 aura shape mapped onto snapshot staleness).
        let healthy: f32 = self
            .neighbor_states
            .values()
            .filter(|n| {
                matches!(
                    n.health,
                    AgentHealth::Operational | AgentHealth::Degraded { .. }
                )
            })
            .map(|n| aoi_weight(n.last_updated.elapsed()))
            .sum();
        // Bounded contagion: the weighted distressed influence is capped at
        // MAX_CONTAGION_DISTRESSED (WH3 friends cap) so panic can't spread
        // unboundedly even in a large formation.
        let distressed: f32 = self
            .neighbor_states
            .values()
            .filter(|n| {
                matches!(
                    n.health,
                    AgentHealth::Incapacitated | AgentHealth::Dead { .. }
                )
            })
            .map(|n| aoi_weight(n.last_updated.elapsed()))
            .sum::<f32>()
            .min(MAX_CONTAGION_DISTRESSED as f32);

        // Base morale from (AoI-weighted) neighbor health ratio.
        let health_factor = healthy / total;
        // Penalty for distressed neighbors (cascade risk).
        let distress_penalty = distressed / total * 0.3;
        // Momentum bonus.
        let momentum_bonus = match self.formation_momentum {
            MomentumTier::Cold => 0.0,
            MomentumTier::Warming => 0.05,
            MomentumTier::Hot => 0.1,
            MomentumTier::Fever => 0.2,
        };

        (health_factor - distress_penalty + momentum_bonus).clamp(0.0, 1.0)
    }

    /// Advance the lerped morale one tick toward [`Self::morale_target`] at
    /// [`MORALE_LERP`], with a [`MORALE_MIN_STEP`] floor so it always converges
    /// (never overshooting). Total War WH3 morale FSM (§A.4) — gradual routing.
    pub fn tick_morale(&mut self) {
        let diff = self.morale_target() - self.morale;
        if diff.abs() <= f32::EPSILON {
            return;
        }
        let mut step = diff * MORALE_LERP;
        if step.abs() < MORALE_MIN_STEP {
            step = MORALE_MIN_STEP.copysign(diff);
        }
        if step.abs() > diff.abs() {
            step = diff; // don't overshoot
        }
        self.morale = (self.morale + step).clamp(0.0, 1.0);
    }

    /// The current (lerped) local morale (0.0–1.0). Stateful — advanced by
    /// [`Self::tick_morale`] each tick rather than recomputed instantaneously.
    pub fn local_morale(&self) -> f32 {
        self.morale
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
        assert!((awareness.morale_target() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_healthy_neighbors() {
        let mut awareness = LocalAwareness::default();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        awareness.update_neighbor(make_snapshot(id1, AgentHealth::Operational));
        awareness.update_neighbor(make_snapshot(id2, AgentHealth::Operational));
        assert_eq!(awareness.healthy_neighbor_count(), 2);
        assert!(awareness.morale_target() > 0.9);
    }

    #[test]
    fn test_distressed_neighbors_lower_morale() {
        let mut awareness = LocalAwareness::default();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        awareness.update_neighbor(make_snapshot(id1, AgentHealth::Operational));
        awareness.update_neighbor(make_snapshot(id2, AgentHealth::Incapacitated));
        assert!(awareness.morale_target() < 0.7);
        assert_eq!(awareness.distressed_neighbor_count(), 1);
    }

    #[test]
    fn test_momentum_boosts_morale() {
        let mut awareness = LocalAwareness::default();
        // Mix of healthy + distressed so morale isn't clamped at 1.0
        awareness.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Operational));
        awareness.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Incapacitated));

        awareness.formation_momentum = MomentumTier::Cold;
        let cold_morale = awareness.morale_target();

        awareness.formation_momentum = MomentumTier::Fever;
        let fever_morale = awareness.morale_target();

        assert!(fever_morale > cold_morale);
    }

    #[test]
    fn morale_lerps_toward_target_gradually() {
        let mut awareness = LocalAwareness::default();
        // All-distressed neighbors ⇒ a low target; morale starts neutral (0.5).
        for _ in 0..3 {
            awareness.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Incapacitated));
        }
        let target = awareness.morale_target();
        assert!(target < 0.5, "distressed neighbors lower the target");

        // One tick moves PART of the way (lerp), not all the way (no snap).
        let before = awareness.local_morale();
        awareness.tick_morale();
        let after = awareness.local_morale();
        assert!(after < before, "morale moved toward the lower target");
        assert!(after > target, "but did not snap to target in one tick");

        // Many ticks converge to the target.
        for _ in 0..100 {
            awareness.tick_morale();
        }
        assert!(
            (awareness.local_morale() - target).abs() < 0.02,
            "converges to target"
        );
    }

    #[test]
    fn morale_contagion_is_bounded() {
        // More than MAX_CONTAGION_DISTRESSED distressed neighbors must not pull
        // the target below what exactly MAX_CONTAGION_DISTRESSED would — panic
        // spread is capped (WH3 friends cap).
        let mut capped = LocalAwareness::default();
        for _ in 0..super::MAX_CONTAGION_DISTRESSED {
            capped.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Incapacitated));
        }
        let cap_target = capped.morale_target();

        let mut over = LocalAwareness::default();
        for _ in 0..(super::MAX_CONTAGION_DISTRESSED + 4) {
            over.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Incapacitated));
        }
        // The penalty numerator is capped, so more distressed neighbors only
        // raise the denominator — the target never drops below the capped case.
        assert!(over.morale_target() >= cap_target);
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
    fn aoi_weight_full_then_linear_to_zero() {
        use std::time::Duration;
        assert!((aoi_weight(Duration::ZERO) - 1.0).abs() < f32::EPSILON);
        assert!((aoi_weight(AOI_FULL_EFFECT) - 1.0).abs() < f32::EPSILON);
        // Midpoint of the falloff band (2s..3s at the defaults) ≈ 0.5.
        let mid = AOI_FULL_EFFECT + Duration::from_millis(500);
        assert!((aoi_weight(mid) - 0.5).abs() < 0.01);
        // At and beyond the zero point: no influence.
        let zero_point = Duration::from_secs_f32(AOI_FULL_EFFECT.as_secs_f32() * AOI_ZERO_FACTOR);
        assert!(aoi_weight(zero_point) <= f32::EPSILON);
        assert!(aoi_weight(zero_point + Duration::from_secs(10)) <= f32::EPSILON);
    }

    #[test]
    fn stale_distressed_neighbor_pulls_morale_less_than_fresh() {
        use std::time::Duration;
        // Fresh distressed neighbor: full contagion weight.
        let mut fresh = LocalAwareness::default();
        fresh.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Operational));
        fresh.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Incapacitated));
        let fresh_target = fresh.morale_target();

        // Same shape, but the distressed snapshot is past the AoI zero
        // point — its influence (healthy AND panic) has faded out.
        let mut stale = LocalAwareness::default();
        stale.update_neighbor(make_snapshot(AgentId::new(), AgentHealth::Operational));
        let mut old = make_snapshot(AgentId::new(), AgentHealth::Incapacitated);
        old.last_updated = Instant::now() - Duration::from_secs(10);
        stale.update_neighbor(old);
        let stale_target = stale.morale_target();

        assert!(
            stale_target > fresh_target,
            "stale distress ({stale_target}) must weigh less than fresh ({fresh_target})"
        );
    }

    #[test]
    fn test_interference_detection() {
        let mut awareness = LocalAwareness::default();
        let my_id = AgentId::new();
        let other_id = AgentId::new();

        let report = TickReport {
            agent_id: other_id,
            tick_sequence: crate::tick::TickId(1),
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
