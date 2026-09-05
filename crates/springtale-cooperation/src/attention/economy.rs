//! Attention economy — workload distribution inspired by Army of Two's aggro.
//!
//! Per COOPERATION.pdf §9: "The aggro meter is a visible pendulum-like gauge.
//! Whichever player has higher aggro draws all enemy attention. The other
//! becomes semi-transparent. One agent's high workload consumption directly
//! enables another's freedom."
//!
//! Attention is zero-sum within a formation. Total attention is fixed.
//! When one agent consumes more (handling requests, processing data),
//! others have freed capacity. Extreme concentration (>90% on one agent)
//! triggers a time-limited power state (Army of Two's Overkill mode).

use std::collections::HashMap;

use crate::cadence::AgentId;

/// Attention economy for a formation — zero-sum workload distribution.
///
/// Like Army of Two's aggro meter: the total is fixed, distribution
/// shifts based on who's doing the most work. High attention on one
/// agent means the others are free to act independently.
#[derive(Debug, Clone)]
pub struct AttentionEconomy {
    /// Total attention budget (sums to 1.0 across all agents).
    total: f32,
    /// Per-agent attention share (values sum to total).
    distribution: HashMap<AgentId, f32>,
}

impl AttentionEconomy {
    /// Create a new attention economy with equal distribution.
    pub fn new(agents: &[AgentId]) -> Self {
        let count = agents.len().max(1) as f32;
        let share = 1.0 / count;
        let distribution = agents.iter().map(|id| (*id, share)).collect();
        Self {
            total: 1.0,
            distribution,
        }
    }

    /// Get an agent's current attention load (0.0-1.0).
    pub fn load(&self, agent_id: &AgentId) -> f32 {
        self.distribution.get(agent_id).copied().unwrap_or(0.0)
    }

    /// Shift attention toward an agent (they're doing more work).
    ///
    /// Amount is redistributed from all other agents proportionally.
    /// Clamped to prevent any agent from going below 0.01.
    pub fn shift_toward(&mut self, agent_id: &AgentId, amount: f32) {
        if !self.distribution.contains_key(agent_id) {
            return;
        }

        let others: Vec<AgentId> = self
            .distribution
            .keys()
            .filter(|id| *id != agent_id)
            .copied()
            .collect();

        if others.is_empty() {
            return;
        }

        let clamped_amount = amount.min(0.3); // max shift per call
        let per_other = clamped_amount / others.len() as f32;

        for other_id in &others {
            if let Some(val) = self.distribution.get_mut(other_id) {
                *val = (*val - per_other).max(0.01);
            }
        }

        // Recalculate target agent's share to maintain sum = 1.0
        let others_total: f32 = self
            .distribution
            .iter()
            .filter(|(id, _)| *id != agent_id)
            .map(|(_, v)| *v)
            .sum();

        if let Some(val) = self.distribution.get_mut(agent_id) {
            *val = (self.total - others_total).max(0.01);
        }
    }

    /// Shift attention away from an agent (they need less load).
    ///
    /// Redistributes the agent's excess load to all other agents.
    /// Inverse of `shift_toward` — explicit API instead of passing
    /// negative amounts (which works by accident but is fragile).
    pub fn shift_away(&mut self, agent_id: &AgentId, amount: f32) {
        if !self.distribution.contains_key(agent_id) {
            return;
        }

        let others: Vec<AgentId> = self
            .distribution
            .keys()
            .filter(|id| *id != agent_id)
            .copied()
            .collect();

        if others.is_empty() {
            return;
        }

        let clamped_amount = amount.abs().min(0.3);
        let per_other = clamped_amount / others.len() as f32;

        // Increase others' share
        for other_id in &others {
            if let Some(val) = self.distribution.get_mut(other_id) {
                *val = (*val + per_other).min(0.99);
            }
        }

        // Recalculate target's share to maintain sum = 1.0
        let others_total: f32 = self
            .distribution
            .iter()
            .filter(|(id, _)| *id != agent_id)
            .map(|(_, v)| *v)
            .sum();

        if let Some(val) = self.distribution.get_mut(agent_id) {
            *val = (self.total - others_total).max(0.01);
        }
    }

    /// Check if an agent is in "overkill" mode (>90% attention).
    ///
    /// Per Army of Two: full aggro triggers fury with multiplied
    /// damage for 15 seconds. The other player becomes transparent
    /// and moves faster. Time-limited cooperative role differentiation.
    pub fn is_overkill(&self, agent_id: &AgentId) -> bool {
        self.load(agent_id) > 0.9
    }

    /// Check if an agent is "free" (<10% attention).
    ///
    /// Low attention means the agent has capacity for independent
    /// action — reconnaissance, recovery, preparation.
    pub fn is_free(&self, agent_id: &AgentId) -> bool {
        self.load(agent_id) < 0.1
    }

    /// Find the agent in overkill mode (>90%), if any.
    pub fn overkill_agent(&self) -> Option<AgentId> {
        self.distribution
            .iter()
            .find(|&(_, v)| *v > 0.9)
            .map(|(id, _)| *id)
    }

    /// Get all agents and their loads.
    pub fn agents(&self) -> &HashMap<AgentId, f32> {
        &self.distribution
    }

    /// Rebalance to equal distribution.
    pub fn rebalance(&mut self) {
        let count = self.distribution.len().max(1) as f32;
        let share = self.total / count;
        for val in self.distribution.values_mut() {
            *val = share;
        }
    }

    /// Add a new agent (splits existing attention).
    pub fn add_agent(&mut self, agent_id: AgentId) {
        self.distribution.insert(agent_id, 0.0);
        self.rebalance();
    }

    /// Remove an agent (redistributes their attention).
    pub fn remove_agent(&mut self, agent_id: &AgentId) {
        self.distribution.remove(agent_id);
        if !self.distribution.is_empty() {
            self.rebalance();
        }
    }

    /// Fold one observed load sample toward the agent's share (EMA), then
    /// renormalize so the economy stays zero-sum. Army of Two: you generate
    /// aggro by firing; here, by working. Idle agents drain toward the floor.
    pub fn observe(&mut self, agent_id: &AgentId, sample: f32, alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        if let Some(v) = self.distribution.get_mut(agent_id) {
            *v = (*v * (1.0 - alpha) + sample.clamp(0.0, 1.0) * alpha).max(0.01);
        }
        self.renormalize();
    }

    /// Renormalize distribution so all values sum to 1.0.
    pub fn renormalize(&mut self) {
        let sum: f32 = self.distribution.values().sum();
        if sum > 0.0 {
            for v in self.distribution.values_mut() {
                *v /= sum;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_distribution() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let economy = AttentionEconomy::new(&agents);
        assert!((economy.load(&agents[0]) - 0.5).abs() < f32::EPSILON);
        assert!((economy.load(&agents[1]) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shift_toward() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);

        economy.shift_toward(&agents[0], 0.2);
        assert!(economy.load(&agents[0]) > 0.6);
        assert!(economy.load(&agents[1]) < 0.4);
    }

    #[test]
    fn test_overkill_threshold() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);

        // Shift heavily toward agent 0
        for _ in 0..5 {
            economy.shift_toward(&agents[0], 0.3);
        }

        assert!(economy.is_overkill(&agents[0]));
        assert!(economy.is_free(&agents[1]));
    }

    #[test]
    fn test_rebalance() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);
        economy.shift_toward(&agents[0], 0.3);

        economy.rebalance();
        assert!((economy.load(&agents[0]) - 0.5).abs() < f32::EPSILON);
        assert!((economy.load(&agents[1]) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_add_agent() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);
        let new_agent = AgentId::new();
        economy.add_agent(new_agent);

        // Should redistribute to thirds
        let expected = 1.0 / 3.0;
        assert!((economy.load(&agents[0]) - expected).abs() < 0.01);
        assert!((economy.load(&new_agent) - expected).abs() < 0.01);
    }

    #[test]
    fn test_remove_agent() {
        let agents = vec![AgentId::new(), AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);
        economy.remove_agent(&agents[2]);

        assert!((economy.load(&agents[0]) - 0.5).abs() < f32::EPSILON);
        assert!((economy.load(&agents[1]) - 0.5).abs() < f32::EPSILON);
        assert!((economy.load(&agents[2])).abs() < f32::EPSILON); // removed
    }

    #[test]
    fn test_unknown_agent() {
        let agents = vec![AgentId::new()];
        let economy = AttentionEconomy::new(&agents);
        let unknown = AgentId::new();
        assert!((economy.load(&unknown)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shift_away_reduces_target() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);

        // Start equal (0.5 each), shift away from agent 0
        economy.shift_away(&agents[0], 0.2);
        assert!(economy.load(&agents[0]) < 0.4);
        assert!(economy.load(&agents[1]) > 0.6);
    }

    #[test]
    fn test_shift_away_increases_others() {
        let agents = vec![AgentId::new(), AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);
        let initial_load = economy.load(&agents[1]);

        economy.shift_away(&agents[0], 0.15);
        assert!(economy.load(&agents[1]) > initial_load);
        assert!(economy.load(&agents[2]) > initial_load);
    }

    #[test]
    fn test_shift_away_unknown_agent_noop() {
        let agents = vec![AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);
        let unknown = AgentId::new();
        let before = economy.load(&agents[0]);
        economy.shift_away(&unknown, 0.3);
        assert!((economy.load(&agents[0]) - before).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shift_away_clamps() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);

        // Shift heavily away — target shouldn't go below 0.01
        for _ in 0..10 {
            economy.shift_away(&agents[0], 0.3);
        }
        assert!(economy.load(&agents[0]) >= 0.01);
    }

    #[test]
    fn test_renormalize_corrects_drift() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let mut economy = AttentionEconomy::new(&agents);
        // Manually corrupt (simulating floating-point drift)
        economy.shift_toward(&agents[0], 0.3);
        economy.shift_toward(&agents[0], 0.3);
        economy.renormalize();
        let sum: f32 = agents.iter().map(|a| economy.load(a)).sum();
        assert!((sum - 1.0).abs() < 0.01);
    }
}
