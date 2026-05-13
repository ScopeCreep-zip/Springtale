//! Momentum system — coherence accumulator inspired by Patapon's Fever (§7).
//!
//! > "Patapon Fever doesn't make units '10% stronger' — it unlocks
//! > attack patterns that don't exist outside Fever. Momentum determines
//! > what agents CAN do." — COOPERATION.md §7
//!
//! Momentum tiers gate cooperation capabilities. Agents must build
//! coherence (consecutive successful ticks with low interference)
//! before accessing advanced cooperative features. This is the opposite
//! of an HP bar — momentum is a permission system, not a damage sink.
//!
//! ## Capability table (§7)
//!
//! | Tier    | Read env | Read neighbors | Chain | Write env | Commit | Consensus | AI | Recruit |
//! |---------|:--------:|:--------------:|:-----:|:---------:|:------:|:---------:|:--:|:-------:|
//! | Cold    | ✓        | —              | —     | —         | —      | —         | —  | —       |
//! | Warming | ✓        | ✓              | ✓     | —         | —      | —         | —  | —       |
//! | Hot     | ✓        | ✓              | ✓     | ✓         | ✓      | —         | —  | —       |
//! | Fever   | ✓        | ✓              | ✓     | ✓         | ✓      | ✓         | ✓  | ✓       |
//!
//! ## Promotion thresholds
//!
//! Promotion requires a minimum run of consecutive successes AND zero
//! interference in the current run:
//!
//! - `Cold → Warming`: 3 consecutive successes
//! - `Warming → Hot`: 8 consecutive successes, 0 interference
//! - `Hot → Fever`: 15 consecutive successes, 0 interference
//!
//! ## Demotion rules
//!
//! - `Fever → Hot`: any interference
//! - `Hot → Warming`: consecutive successes drop below 5
//! - `Warming → Cold`: consecutive successes reach 0
//!
//! Interference resets `interference_count` on demotion so the formation
//! is measured on its *post-demotion* behavior, not the incident that
//! dropped it.
//!
//! ## Trust decay
//!
//! Per Microsoft Agent Governance Toolkit research: trust must decay
//! without positive signals. `check_decay` enforces two rules:
//!
//! 1. **Success-counter decay** — one success decayed per `decay_interval`
//!    of inactivity. This alone isn't enough, because a tier whose
//!    success count is already at 0 can idle forever.
//! 2. **Forced demotion** — after `3 × decay_interval` of inactivity,
//!    force-demote one tier regardless of counter. Catches the "Hot with
//!    0 successes, idle forever" case.
//!
//! `last_activity` is refreshed only by `record_activity` (real work
//! happened) — NOT by `record_success` (tick alignment without work).
//! This is important: a formation whose members all report "aligned but
//! nothing to do" should still decay.
//!
//! ## Why an FSM, not a score
//!
//! An earlier draft was a single `coherence: f32` with threshold
//! functions. Two problems: (a) cliff effects at threshold boundaries
//! caused the UI to flicker, and (b) there was no natural place to hang
//! capability gates. The discrete-tier FSM gives stable UI, clear audit
//! logs ("formation X promoted Warming→Hot"), and a single place — the
//! `can_*` methods — to check whether a given capability is available.

pub mod authority_impl;

use std::time::Instant;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Typed event that drives momentum transitions (spec §7).
///
/// Instead of callers picking `record_success()` vs `record_failure()` ad hoc,
/// the tick pipeline builds a `MomentumEvent` and hands it to
/// `MomentumState::apply_event()`. This makes the FSM's input language
/// explicit and exhaustive.
#[derive(Debug, Clone)]
pub enum MomentumEvent {
    TickSuccess { had_real_action: bool },
    TickInterference { count: u32 },
    TickFailure,
    IntentChanged(crate::cadence::IntentPattern),
}

/// Momentum tier — determines what cooperative capabilities are available.
///
/// | Tier    | Read env | Read neighbors | Chain | Write env | Commit | Consensus | AI | Recruit |
/// |---------|----------|---------------|-------|-----------|--------|-----------|-----|---------|
/// | Cold    | ✓        | —             | —     | —         | —      | —         | —   | —       |
/// | Warming | ✓        | ✓             | ✓     | —         | —      | —         | —   | —       |
/// | Hot     | ✓        | ✓             | ✓     | ✓         | ✓      | —         | —   | —       |
/// | Fever   | ✓        | ✓             | ✓     | ✓         | ✓      | ✓         | ✓   | ✓       |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
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

impl MomentumTier {
    /// Parse a tier from its Debug string representation (as stored in DB).
    pub fn parse(s: &str) -> Self {
        match s {
            "Warming" => Self::Warming,
            "Hot" => Self::Hot,
            "Fever" => Self::Fever,
            _ => Self::Cold,
        }
    }
}

/// Current momentum state for a formation.
///
/// Per Microsoft Agent Governance Toolkit research: trust should decay
/// without positive signals. An idle formation shouldn't retain Fever
/// indefinitely — `check_decay()` handles this.
#[derive(Debug, Clone)]
pub struct MomentumState {
    pub tier: MomentumTier,
    pub consecutive_successes: u32,
    pub interference_count: u32,
    pub last_transition: Option<Instant>,
    /// When the last successful tick was recorded. Used for trust decay —
    /// idle formations lose momentum over time (Microsoft AGT pattern).
    pub last_activity: Instant,
    /// How long a formation can be idle before momentum decays one step.
    /// Default: 60 seconds. Configurable per formation.
    pub decay_interval: std::time::Duration,
    /// Last known intent — tracked for IntentChanged events.
    pub last_intent: Option<crate::cadence::IntentPattern>,
    /// Number of ticks the formation has spent stuck in Cold tier without
    /// promoting. Drives the `cold_duration_ticks` signal that the L6
    /// intervention layer (`orchestrator/intervention/`) reads to decide
    /// when to `EscalateToUser` (default threshold 700 ticks). Incremented
    /// every tick while in Cold; reset on every promotion out of Cold.
    pub cold_ticks: u32,
}

impl Default for MomentumState {
    fn default() -> Self {
        Self {
            tier: MomentumTier::Cold,
            consecutive_successes: 0,
            interference_count: 0,
            last_transition: None,
            last_activity: Instant::now(),
            decay_interval: std::time::Duration::from_secs(60),
            last_intent: None,
            cold_ticks: 0,
        }
    }
}

impl MomentumState {
    /// Record a successful tick. May promote tier.
    ///
    /// NOTE: Does NOT refresh last_activity. Only `record_activity()`
    /// refreshes it — this prevents decay from being a no-op when
    /// ticks fire but no real work happens.
    pub fn record_success(&mut self) {
        self.consecutive_successes += 1;
        self.try_promote();
    }

    /// Record that agents actually did work (connector executed, action completed).
    ///
    /// Separate from `record_success()` because tick success just means
    /// "all members reported alignment > 0.5" — it doesn't mean actual
    /// connector actions happened. Decay tracks real activity, not ticks.
    pub fn record_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Record interference. May demote tier.
    pub fn record_interference(&mut self) {
        self.interference_count += 1;
        self.consecutive_successes = self.consecutive_successes.saturating_sub(2);
        self.last_activity = Instant::now(); // interference IS activity (bad activity)
        self.try_demote();
    }

    /// Record a failed tick. Resets consecutive count, may demote.
    pub fn record_failure(&mut self) {
        self.consecutive_successes = 0;
        self.try_demote();
    }

    /// Check for time-based momentum decay.
    ///
    /// Per Microsoft Agent Governance Toolkit: trust should decay without
    /// positive signals. An idle formation loses momentum over time.
    /// Called once per cadence tick.
    ///
    /// Two decay modes:
    /// 1. Success counter decay: one per decay_interval of inactivity
    /// 2. Forced tier demotion: after 3x decay_interval with no activity,
    ///    force demotion regardless of success count (handles the
    ///    "Hot with 0 successes but idle" case)
    pub fn check_decay(&mut self) {
        // L6 intervention signal — count ticks stuck in Cold so the
        // intervention evaluator can decide when to escalate to user.
        if self.tier == MomentumTier::Cold {
            self.cold_ticks = self.cold_ticks.saturating_add(1);
            return; // nothing to decay
        }

        let elapsed = self.last_activity.elapsed();
        if elapsed < self.decay_interval {
            return; // recent activity, no decay
        }

        // Mode 1: Decay success counter
        if self.consecutive_successes > 0 {
            let intervals = (elapsed.as_secs() / self.decay_interval.as_secs().max(1)) as u32;
            let decay = intervals.min(self.consecutive_successes);
            self.consecutive_successes = self.consecutive_successes.saturating_sub(decay);

            tracing::debug!(
                tier = ?self.tier,
                decayed = decay,
                remaining = self.consecutive_successes,
                "momentum decaying from inactivity"
            );
            self.try_demote();
        }

        // Mode 2: Force demotion after extended inactivity (3x interval)
        // Handles: Hot tier with 0 successes, idle forever
        if elapsed >= self.decay_interval * 3 && self.tier != MomentumTier::Cold {
            let old_tier = self.tier;
            self.tier = match self.tier {
                MomentumTier::Fever => MomentumTier::Hot,
                MomentumTier::Hot => MomentumTier::Warming,
                MomentumTier::Warming => MomentumTier::Cold,
                MomentumTier::Cold => MomentumTier::Cold,
            };
            if self.tier != old_tier {
                self.last_transition = Some(Instant::now());
                tracing::info!(
                    from = ?old_tier,
                    to = ?self.tier,
                    "momentum force-demoted from extended inactivity"
                );
            }
        }
    }

    fn try_promote(&mut self) {
        let old_tier = self.tier;
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
        // Promotion out of Cold resets the L6 intervention counter — the
        // formation has shown signs of life so the "stuck in Cold" alarm
        // restarts.
        self.cold_ticks = 0;
        tracing::info!(
            from = ?old_tier,
            to = ?new_tier,
            successes = self.consecutive_successes,
            "momentum promoted"
        );
    }

    fn try_demote(&mut self) {
        let old_tier = self.tier;
        let new_tier = match self.tier {
            MomentumTier::Fever if self.interference_count > 0 => MomentumTier::Hot,
            MomentumTier::Hot if self.consecutive_successes < 5 => MomentumTier::Warming,
            MomentumTier::Warming if self.consecutive_successes == 0 => MomentumTier::Cold,
            _ => return,
        };
        self.tier = new_tier;
        self.interference_count = 0;
        self.last_transition = Some(Instant::now());
        tracing::info!(
            from = ?old_tier,
            to = ?new_tier,
            successes = self.consecutive_successes,
            "momentum demoted"
        );
    }

    /// Check if a capability is available at the current tier.
    pub fn can_read_neighbor_state(&self) -> bool {
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

    pub fn can_ai_orchestrate(&self) -> bool {
        self.tier >= MomentumTier::Fever
    }

    pub fn can_recruit(&self) -> bool {
        self.tier >= MomentumTier::Fever
    }

    /// Typed event driver — translates a `MomentumEvent` into the appropriate
    /// state mutations. Callers build the event from tick results; this method
    /// is the single dispatch point so the FSM's behavior is auditable from
    /// one match arm.
    pub fn apply_event(&mut self, event: &MomentumEvent) {
        match event {
            MomentumEvent::TickSuccess { had_real_action } => {
                self.record_success();
                if *had_real_action {
                    self.record_activity();
                }
            }
            MomentumEvent::TickInterference { count } => {
                for _ in 0..*count {
                    self.record_interference();
                }
            }
            MomentumEvent::TickFailure => {
                self.record_failure();
            }
            MomentumEvent::IntentChanged(_new_intent) => {
                self.consecutive_successes = 0;
            }
        }
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
        assert!(!state.can_read_neighbor_state());
        assert!(!state.can_chain());
    }

    #[test]
    fn test_promote_to_warming() {
        let mut state = MomentumState::default();
        for _ in 0..3 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Warming);
        assert!(state.can_read_neighbor_state());
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
        assert!(state.can_ai_orchestrate());
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

    #[test]
    fn test_decay_reduces_successes() {
        let mut state = MomentumState {
            decay_interval: std::time::Duration::from_millis(1),
            ..MomentumState::default()
        };
        for _ in 0..5 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Warming);

        // Simulate inactivity by backdating last_activity
        state.last_activity = Instant::now() - std::time::Duration::from_secs(5);
        state.check_decay();

        // Successes should have decayed, potentially demoting
        assert!(state.consecutive_successes < 5);
    }

    #[test]
    fn test_cold_does_not_decay() {
        let mut state = MomentumState {
            decay_interval: std::time::Duration::from_millis(1),
            last_activity: Instant::now() - std::time::Duration::from_secs(100),
            ..MomentumState::default()
        };
        state.check_decay();
        assert_eq!(state.tier, MomentumTier::Cold);
        assert_eq!(state.consecutive_successes, 0);
    }

    #[test]
    fn test_recent_activity_does_not_decay() {
        let mut state = MomentumState::default();
        for _ in 0..8 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Hot);

        // last_activity is fresh (just recorded success), no decay
        state.check_decay();
        assert_eq!(state.tier, MomentumTier::Hot);
        assert_eq!(state.consecutive_successes, 8);
    }
}
