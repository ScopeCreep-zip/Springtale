//! Momentum system — coherence accumulator inspired by Patapon's Fever.
//!
//! Per COOPERATION.pdf §7: "Patapon Fever doesn't make units '10% stronger' —
//! it unlocks attack patterns that don't exist outside Fever. Momentum
//! determines what agents CAN do."
//!
//! Momentum tiers gate cooperation capabilities. Agents must build
//! coherence (consecutive successful ticks with low interference)
//! before accessing advanced cooperative features.

use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Momentum tier — determines what cooperative capabilities are available.
///
/// | Tier    | Read env | Read neighbors | Chain | Write env | Commit | Consensus | AI | Recruit |
/// |---------|----------|---------------|-------|-----------|--------|-----------|-----|---------|
/// | Cold    | ✓        | —             | —     | —         | —      | —         | —   | —       |
/// | Warming | ✓        | ✓             | ✓     | —         | —      | —         | —   | —       |
/// | Hot     | ✓        | ✓             | ✓     | ✓         | ✓      | —         | —   | —       |
/// | Fever   | ✓        | ✓             | ✓     | ✓         | ✓      | ✓         | ✓   | ✓       |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MomentumTier {
    /// Just assembled. Read-only environment. No chaining.
    Cold,
    /// 3+ successful ticks. Can read neighbor reports. Basic chaining.
    Warming,
    /// 8+ successful ticks, low interference. Write to environment. Synchronized commit.
    Hot,
    /// 15+ successful ticks, zero interference. Consensus unlocked. AI adapter access. Recruit.
    Fever,
}

/// Current momentum state for a formation.
#[derive(Debug, Clone)]
pub struct MomentumState {
    pub tier: MomentumTier,
    pub consecutive_successes: u32,
    pub interference_count: u32,
    pub last_transition: Option<Instant>,
}

impl Default for MomentumState {
    fn default() -> Self {
        Self {
            tier: MomentumTier::Cold,
            consecutive_successes: 0,
            interference_count: 0,
            last_transition: None,
        }
    }
}

impl MomentumState {
    /// Record a successful tick. May promote tier.
    pub fn record_success(&mut self) {
        self.consecutive_successes += 1;
        self.try_promote();
    }

    /// Record interference. May demote tier.
    pub fn record_interference(&mut self) {
        self.interference_count += 1;
        self.consecutive_successes = self.consecutive_successes.saturating_sub(2);
        self.try_demote();
    }

    /// Record a failed tick. Resets consecutive count, may demote.
    pub fn record_failure(&mut self) {
        self.consecutive_successes = 0;
        self.try_demote();
    }

    fn try_promote(&mut self) {
        let new_tier = match self.tier {
            MomentumTier::Cold if self.consecutive_successes >= 3 => MomentumTier::Warming,
            MomentumTier::Warming
                if self.consecutive_successes >= 8 && self.interference_count == 0 =>
            {
                MomentumTier::Hot
            }
            MomentumTier::Hot
                if self.consecutive_successes >= 15 && self.interference_count == 0 =>
            {
                MomentumTier::Fever
            }
            _ => return,
        };
        self.tier = new_tier;
        self.last_transition = Some(Instant::now());
    }

    fn try_demote(&mut self) {
        let new_tier = match self.tier {
            MomentumTier::Fever if self.interference_count > 0 => MomentumTier::Hot,
            MomentumTier::Hot if self.consecutive_successes < 5 => MomentumTier::Warming,
            MomentumTier::Warming if self.consecutive_successes == 0 => MomentumTier::Cold,
            _ => return,
        };
        self.tier = new_tier;
        self.interference_count = 0;
        self.last_transition = Some(Instant::now());
    }

    /// Check if a capability is available at the current tier.
    pub fn can_read_neighbors(&self) -> bool {
        self.tier >= MomentumTier::Warming
    }

    pub fn can_chain(&self) -> bool {
        self.tier >= MomentumTier::Warming
    }

    pub fn can_write_environment(&self) -> bool {
        self.tier >= MomentumTier::Hot
    }

    pub fn can_synchronized_commit(&self) -> bool {
        self.tier >= MomentumTier::Hot
    }

    pub fn can_consensus(&self) -> bool {
        self.tier >= MomentumTier::Fever
    }

    pub fn can_use_ai(&self) -> bool {
        self.tier >= MomentumTier::Fever
    }

    pub fn can_recruit(&self) -> bool {
        self.tier >= MomentumTier::Fever
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_cold_start() {
        let state = MomentumState::default();
        assert_eq!(state.tier, MomentumTier::Cold);
        assert!(!state.can_read_neighbors());
        assert!(!state.can_chain());
    }

    #[test]
    fn test_promote_to_warming() {
        let mut state = MomentumState::default();
        for _ in 0..3 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Warming);
        assert!(state.can_read_neighbors());
        assert!(state.can_chain());
        assert!(!state.can_write_environment());
    }

    #[test]
    fn test_promote_to_hot() {
        let mut state = MomentumState::default();
        for _ in 0..8 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Hot);
        assert!(state.can_write_environment());
        assert!(state.can_synchronized_commit());
        assert!(!state.can_consensus());
    }

    #[test]
    fn test_promote_to_fever() {
        let mut state = MomentumState::default();
        for _ in 0..15 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Fever);
        assert!(state.can_consensus());
        assert!(state.can_use_ai());
        assert!(state.can_recruit());
    }

    #[test]
    fn test_interference_prevents_promotion() {
        let mut state = MomentumState::default();
        for _ in 0..7 {
            state.record_success();
        }
        state.record_interference();
        // Should not promote to Hot because interference_count > 0
        state.record_success();
        assert_eq!(state.tier, MomentumTier::Warming);
    }

    #[test]
    fn test_failure_resets_consecutive() {
        let mut state = MomentumState::default();
        for _ in 0..5 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Warming);
        state.record_failure();
        assert_eq!(state.consecutive_successes, 0);
        assert_eq!(state.tier, MomentumTier::Cold);
    }

    #[test]
    fn test_fever_demotes_on_interference() {
        let mut state = MomentumState::default();
        for _ in 0..15 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Fever);
        state.record_interference();
        assert_eq!(state.tier, MomentumTier::Hot);
    }
}
