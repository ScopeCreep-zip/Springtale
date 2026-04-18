//! Rally & cascade recovery — formation self-healing before orchestrator escalation.
//!
//! Per COOPERATION.pdf §15:
//! Game sources: Total War general rally, routing cascade, Monster Hunter carts.
//!
//! §15.1 Cascade Detection: Agent A fails → neighbors see it → their health drops → cascade risk.
//! §15.2 Formation Self-Rally (before escalating to orchestrator):
//!   1. Redistribute attention (§9) away from struggling agent
//!   2. Transform roles (§14) for failed agent
//!   3. Reduce momentum tier to match reduced coherence
//!   4. Consume rally token (limited, like Monster Hunter carts)
//!
//! §15.3 Escalation: Only if self-rally fails (tokens consumed, Cold momentum,
//! multiple agents failing) does the formation escalate to orchestrator::intervention.

/// Rally attempt result.
pub enum RallyResult {
    /// Formation self-recovered successfully.
    Recovered,
    /// Rally token consumed but formation stabilized.
    StabilizedWithCost { tokens_remaining: u32 },
    /// Self-rally failed — escalate to orchestrator::intervention.
    EscalateToOrchestrator { reason: String },
}

/// Lifecycle events during a rally attempt (spec §15).
///
/// Used by the cascade detector and the event-loop's rally handler to
/// drive logging, state transitions, and escalation decisions.
#[derive(Debug, Clone)]
pub enum RallyEvent {
    /// An agent went down — rally eligible.
    PeerDown { agent: crate::cadence::AgentId },
    /// Attention was redistributed away from a failing agent.
    AttentionRedistributed { from: crate::cadence::AgentId },
    /// A role transformation was applied as part of self-recovery.
    RoleTransformed { agent: crate::cadence::AgentId },
    /// A rally token was consumed.
    TokenConsumed { remaining: u32 },
    /// Self-rally exhausted — escalating to orchestrator intervention.
    Escalated { reason: String },
}

/// Rally state tracked per formation.
pub struct RallyState {
    /// Limited rally tokens (Monster Hunter carts — 3 deaths = hunt failed).
    pub tokens_remaining: u32,
    /// Maximum tokens available.
    pub max_tokens: u32,
}

impl Default for RallyState {
    fn default() -> Self {
        Self {
            tokens_remaining: 3,
            max_tokens: 3,
        }
    }
}

impl RallyState {
    /// Attempt to consume a rally token.
    pub fn consume_token(&mut self) -> bool {
        if self.tokens_remaining > 0 {
            self.tokens_remaining -= 1;
            true
        } else {
            false
        }
    }

    /// Check if the formation can still self-rally.
    pub fn can_rally(&self) -> bool {
        self.tokens_remaining > 0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_rally_tokens() {
        let mut state = RallyState::default();
        assert!(state.can_rally());
        assert!(state.consume_token());
        assert!(state.consume_token());
        assert!(state.consume_token());
        assert!(!state.consume_token()); // exhausted
        assert!(!state.can_rally());
    }
}
