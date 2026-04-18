//! AttentionBroker — concurrent ArcSwap wrapper around AttentionEconomy.
//!
//! Per COOPERATION.md §9.2: attention is read every tick by every agent,
//! written occasionally. ArcSwap is the canonical primitive for this
//! read-dominated workload. `rcu` provides lock-free copy-on-write updates.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::cadence::AgentId;

use super::economy::AttentionEconomy;

/// Thread-safe broker for the formation's attention economy.
///
/// Agents call `current()` on every tick (lock-free read via ArcSwap::load).
/// Mutations go through `absorb()` / `release()` which use `rcu` (read-copy-update)
/// to atomically swap the underlying `AttentionEconomy`.
pub struct AttentionBroker {
    state: ArcSwap<AttentionEconomy>,
}

impl AttentionBroker {
    /// Create a broker from an initial attention economy.
    pub fn new(economy: AttentionEconomy) -> Self {
        Self {
            state: ArcSwap::from_pointee(economy),
        }
    }

    /// Create a broker with equal distribution for the given agents.
    pub fn for_agents(agents: &[AgentId]) -> Self {
        Self::new(AttentionEconomy::new(agents))
    }

    /// Read the current attention state (lock-free).
    pub fn current(&self) -> Arc<AttentionEconomy> {
        self.state.load_full()
    }

    /// Shift attention toward an agent (they absorbed work).
    ///
    /// Per Army of Two: taking an action draws aggro. The agent
    /// becomes the focus, freeing others.
    pub fn absorb(&self, agent: AgentId, delta: f32) {
        self.state.rcu(|prev| {
            let mut new = (**prev).clone();
            new.shift_toward(&agent, delta);
            Arc::new(new)
        });
    }

    /// Shift attention away from an agent (they released work).
    pub fn release(&self, agent: AgentId, delta: f32) {
        self.state.rcu(|prev| {
            let mut new = (**prev).clone();
            new.shift_away(&agent, delta);
            Arc::new(new)
        });
    }

    /// Army of Two Overkill trigger: returns the agent with >90% concentration.
    pub fn in_overkill(&self) -> Option<AgentId> {
        self.state.load().overkill_agent()
    }

    /// Add an agent to the economy.
    pub fn add_agent(&self, agent: AgentId) {
        self.state.rcu(|prev| {
            let mut new = (**prev).clone();
            new.add_agent(agent);
            Arc::new(new)
        });
    }

    /// Remove an agent from the economy.
    pub fn remove_agent(&self, agent: &AgentId) {
        self.state.rcu(|prev| {
            let mut new = (**prev).clone();
            new.remove_agent(agent);
            Arc::new(new)
        });
    }

    /// Rebalance all agents to equal distribution.
    pub fn rebalance(&self) {
        self.state.rcu(|prev| {
            let mut new = (**prev).clone();
            new.rebalance();
            Arc::new(new)
        });
    }
}

impl std::fmt::Debug for AttentionBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttentionBroker")
            .field("state", &*self.state.load())
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn broker_reads_current_state() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);
        let snapshot = broker.current();
        assert!((snapshot.load(&agents[0]) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn absorb_shifts_toward_agent() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);

        broker.absorb(agents[0], 0.2);
        let snapshot = broker.current();
        assert!(snapshot.load(&agents[0]) > 0.6);
        assert!(snapshot.load(&agents[1]) < 0.4);
    }

    #[test]
    fn release_shifts_away_from_agent() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);

        broker.release(agents[0], 0.2);
        let snapshot = broker.current();
        assert!(snapshot.load(&agents[0]) < 0.4);
        assert!(snapshot.load(&agents[1]) > 0.6);
    }

    #[test]
    fn add_agent_rebalances() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);
        let new_agent = AgentId::new();
        broker.add_agent(new_agent);

        let snapshot = broker.current();
        let expected = 1.0 / 3.0;
        assert!((snapshot.load(&agents[0]) - expected).abs() < 0.01);
        assert!((snapshot.load(&new_agent) - expected).abs() < 0.01);
    }

    #[test]
    fn remove_agent_rebalances() {
        let agents = vec![AgentId::new(), AgentId::new(), AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);
        broker.remove_agent(&agents[2]);

        let snapshot = broker.current();
        assert!((snapshot.load(&agents[0]) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn rebalance_equalizes() {
        let agents = vec![AgentId::new(), AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);
        broker.absorb(agents[0], 0.3);
        broker.rebalance();

        let snapshot = broker.current();
        assert!((snapshot.load(&agents[0]) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn broker_is_debuggable() {
        let agents = vec![AgentId::new()];
        let broker = AttentionBroker::for_agents(&agents);
        let debug = format!("{broker:?}");
        assert!(debug.contains("AttentionBroker"));
    }
}
