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
//! Promotion measures cooperation, not tick count. Every tick adds its
//! [`TickCounts`] to the [`RunWindow`] of the current clean run and
//! `try_promote` checks the [`TierThreshold`] row for the next tier
//! against the window's *rates*, not against how many ticks have passed:
//!
//! | Step             | min actions | min success | max duplicate |
//! |------------------|:-----------:|:-----------:|:-------------:|
//! | `Cold → Warming` | 3           | 0.80        | 1.00          |
//! | `Warming → Hot`  | 8           | 0.90        | 0.30          |
//! | `Hot → Fever`    | 15          | 0.95        | 0.10          |
//!
//! There is no interference column: an interference restarts the window,
//! so its rate is always zero when promotion is checked. Interference is
//! enforced by the Patapon rule instead (below).
//!
//! These numbers are Springtale's own starting values, not taken from any
//! game or paper. Overcooked-AI (Carroll et al., 2019) scores coordination
//! by throughput and by how often partners duplicate or block each other,
//! never by how long they have played; the *shape* of the measure follows
//! that, the values do not. They live in [`MomentumConfig`] so they can
//! be tuned after play, not before (COOPERATION.md A.1.1, A.4.1).
//!
//! `consecutive_successes` stays for the UI's "successes to next tier"
//! hint and for persistence; it no longer decides promotion.
//!
//! ## Demotion rules
//!
//! Symmetric with promotion. Once the window holds the current tier's
//! `min_actions`, a `success_rate` below that row's `min_success` or a
//! `duplicate_rate` above its `max_duplicate` demotes one step and
//! restarts the window. Two Patapon rules sit on top:
//!
//! - `Fever → Hot`: any interference (the run restarts)
//! - `Fever → Hot`: any failed tick (a "Good" breaks Fever)
//!
//! A single failed tick below Fever does not demote by itself; it lowers
//! the run's success rate. Forced decay demotion is separate (below).
//!
//! Interference breaks the combo (Patapon: any miss ends the run and the
//! counter restarts from zero). `record_interference` demotes Fever → Hot,
//! then resets `consecutive_successes`, the per-run `interference_count`
//! and the [`RunWindow`] so a new clean run starts at once; the lifetime total lives in
//! `interference_total`. A past interference never blocks promotion.
//!
//! A tick where nobody acted is `MomentumEvent::TickIdle`: no counter
//! change either way. Per the Microsoft AGT trust calibration, "idle time
//! cannot raise scores" — only the decay clock keeps running.
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
//!    0 successes, idle forever" case. At most one step per
//!    `decay_interval`: a transition inside the current interval defers
//!    the next forced step, so decay cannot cascade in one call.
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
    /// At least one member acted and nothing failed or interfered. The
    /// counts feed the [`RunWindow`].
    TickSuccess {
        counts: TickCounts,
    },
    /// Nobody acted. Not a success, not a failure. The decay clock runs.
    TickIdle,
    /// One or more interference events between members this tick.
    TickInterference {
        count: u32,
    },
    /// At least one member acted and misaligned. The counts still feed
    /// the window (`successes < actions`), so one bad action in a long
    /// clean run lowers the success rate instead of erasing the run.
    TickFailure {
        counts: TickCounts,
    },
    IntentChanged(crate::cadence::IntentPattern),
}

/// What one tick contributes to the [`RunWindow`]. Built by the bot
/// runtime's `update_momentum::classify` from the tick's reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickCounts {
    /// Reports with `action_taken: Some(_)`.
    pub actions: u32,
    /// Of those, reports with `intent_alignment > 0.5`.
    pub successes: u32,
    /// Acted reports whose `ActionDescriptor` `(kind, target, payload_hash)`
    /// repeats an earlier report's in the same tick: two members doing the
    /// same work.
    pub duplicates: u32,
    /// Handoffs completed this tick, all outcomes.
    pub handoffs: u32,
    /// Handoffs completed this tick that succeeded.
    pub handoffs_ok: u32,
}

impl TickCounts {
    /// One member acted once, cleanly. The unit `record_success` adds.
    pub const fn single_success() -> Self {
        Self {
            actions: 1,
            successes: 1,
            duplicates: 0,
            handoffs: 0,
            handoffs_ok: 0,
        }
    }
}

/// Counts over the current clean run. Reset with the combo on
/// interference and intent change; a failed tick stays in the window so
/// its misaligned actions lower `success_rate`.
#[derive(Debug, Clone, Default)]
pub struct RunWindow {
    pub ticks: u32,
    pub actions: u32,
    pub successes: u32,
    pub interferences: u32,
    pub handoffs: u32,
    pub handoffs_ok: u32,
    pub duplicate_actions: u32,
}

impl RunWindow {
    fn ratio(n: u32, d: u32) -> f32 {
        if d == 0 { 0.0 } else { n as f32 / d as f32 }
    }
    /// Successful actions over all actions.
    pub fn success_rate(&self) -> f32 {
        Self::ratio(self.successes, self.actions)
    }
    /// Interference events over ticks.
    pub fn interference_rate(&self) -> f32 {
        Self::ratio(self.interferences, self.ticks)
    }
    /// Duplicate actions over all actions.
    pub fn duplicate_rate(&self) -> f32 {
        Self::ratio(self.duplicate_actions, self.actions)
    }
    /// Successful handoffs over all handoffs.
    pub fn handoff_rate(&self) -> f32 {
        Self::ratio(self.handoffs_ok, self.handoffs)
    }

    /// Add one tick's counts.
    pub fn absorb(&mut self, counts: &TickCounts) {
        self.ticks = self.ticks.saturating_add(1);
        self.actions = self.actions.saturating_add(counts.actions);
        self.successes = self.successes.saturating_add(counts.successes);
        self.duplicate_actions = self.duplicate_actions.saturating_add(counts.duplicates);
        self.handoffs = self.handoffs.saturating_add(counts.handoffs);
        self.handoffs_ok = self.handoffs_ok.saturating_add(counts.handoffs_ok);
    }

    /// Whether this window clears a promotion threshold.
    pub fn clears(&self, threshold: &TierThreshold) -> bool {
        self.actions >= threshold.min_actions
            && self.success_rate() >= threshold.min_success
            && self.duplicate_rate() <= threshold.max_duplicate
    }

    /// Whether this window falls below a row it once cleared: enough
    /// actions to judge, and a rate on the wrong side.
    pub fn fails(&self, threshold: &TierThreshold) -> bool {
        self.actions >= threshold.min_actions
            && (self.success_rate() < threshold.min_success
                || self.duplicate_rate() > threshold.max_duplicate)
    }
}

/// One row of the promotion table: what a run must show before the next
/// tier opens, and must keep showing to hold it. Rates are over the
/// [`RunWindow`]. No `max_interference`: an interference restarts the
/// window, so its rate is always zero at promotion time; interference is
/// enforced by the Patapon rule (breaks the run, demotes Fever) instead.
#[derive(Debug, Clone, Deserialize)]
pub struct TierThreshold {
    pub min_actions: u32,
    pub min_success: f32,
    pub max_duplicate: f32,
}

/// `[cooperation.momentum]` in springtale.toml. Defaults are Springtale's
/// own starting numbers, not from any game. They are configuration, not
/// constants, for the same reason Left 4 Dead ships every Director number
/// as a cvar or `DirectorOptions` field (COOPERATION.md A.1.1) and Total War
/// keeps its morale and fatigue numbers in database tables (A.4.1): tuning
/// happens after play, not before.
#[derive(Debug, Clone, Deserialize)]
pub struct MomentumConfig {
    /// Rows for `Cold → Warming`, `Warming → Hot`, `Hot → Fever`.
    pub promote: [TierThreshold; 3],
}

impl Default for MomentumConfig {
    fn default() -> Self {
        Self {
            promote: [
                // Cold → Warming
                TierThreshold {
                    min_actions: 3,
                    min_success: 0.80,
                    max_duplicate: 1.00,
                },
                // Warming → Hot
                TierThreshold {
                    min_actions: 8,
                    min_success: 0.90,
                    max_duplicate: 0.30,
                },
                // Hot → Fever
                TierThreshold {
                    min_actions: 15,
                    min_success: 0.95,
                    max_duplicate: 0.10,
                },
            ],
        }
    }
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
    /// 3+ actions at 80% success. Can read neighbor reports. Basic chaining.
    Warming,
    /// 8+ actions at 90% success, <=30% duplicate work. Write to environment. Synchronized commit.
    Hot,
    /// 15+ actions at 95% success, <=10% duplicate work. Consensus unlocked. AI adapter access. Recruit.
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
    /// Interferences inside the current clean run. Always 0 between events;
    /// kept for persistence and the UI.
    pub interference_count: u32,
    /// Lifetime count, for the UI and the mental model.
    pub interference_total: u32,
    /// Counts over the current clean run. `try_promote` reads its rates.
    pub window: RunWindow,
    /// Promotion table. Per formation; see [`MomentumConfig`].
    pub config: MomentumConfig,
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
            interference_total: 0,
            window: RunWindow::default(),
            config: MomentumConfig::default(),
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
        self.record_successful_tick(&TickCounts::single_success());
    }

    /// A state with its own promotion table (per formation).
    pub fn with_config(config: MomentumConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    /// Record a successful tick with its counts: feeds the window, extends
    /// the combo, may promote.
    pub fn record_successful_tick(&mut self, counts: &TickCounts) {
        self.window.absorb(counts);
        self.consecutive_successes += 1;
        if !self.try_promote() {
            self.try_demote();
        }
    }

    /// Record that agents actually did work (connector executed, action completed).
    ///
    /// Separate from `record_success()` because tick success just means
    /// "all members reported alignment > 0.5" — it doesn't mean actual
    /// connector actions happened. Decay tracks real activity, not ticks.
    pub fn record_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Record interference. Demotes Fever → Hot, then breaks the combo.
    ///
    /// Patapon: any miss ends the run and the counter restarts from zero.
    /// The formation can rebuild toward Hot/Fever immediately; a past
    /// interference never permanently blocks promotion.
    pub fn record_interference(&mut self) {
        self.interference_count += 1;
        self.interference_total = self.interference_total.saturating_add(1);
        self.last_activity = Instant::now(); // interference IS activity (bad activity)
        if self.tier == MomentumTier::Fever {
            self.demote_to(MomentumTier::Hot, "interference breaks Fever");
        }
        // The combo is broken. A new clean run starts now.
        self.consecutive_successes = 0;
        self.interference_count = 0;
        self.window = RunWindow::default();
    }

    /// Record a failed tick. Resets the consecutive count. A failed tick
    /// breaks Fever (Patapon: a "Good" ends Fever); below Fever it only
    /// lowers the run's success rate, and `try_demote` judges the rate.
    pub fn record_failure(&mut self) {
        self.consecutive_successes = 0;
        if self.tier == MomentumTier::Fever {
            self.demote_to(MomentumTier::Hot, "a failed tick breaks Fever");
        } else {
            self.try_demote();
        }
    }

    /// Record a failed tick with its counts. The window keeps the tick so
    /// its misaligned actions lower `success_rate`; the combo breaks and
    /// the tier may demote.
    pub fn record_failed_tick(&mut self, counts: &TickCounts) {
        self.window.absorb(counts);
        self.record_failure();
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
    ///    "Hot with 0 successes but idle" case). At most one step per
    ///    decay_interval — decay cannot fight promotion or cascade.
    ///    Forced demotion also resets the `RunWindow`.
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
        // Handles: Hot tier with 0 successes, idle forever.
        // Guard: only when the last transition (either direction) is older
        // than one decay_interval, so forced demotion is one step per
        // interval and never cascades within a single call.
        let recently_transitioned = self
            .last_transition
            .is_some_and(|t| t.elapsed() < self.decay_interval);
        if elapsed >= self.decay_interval * 3
            && self.tier != MomentumTier::Cold
            && !recently_transitioned
        {
            let old_tier = self.tier;
            self.tier = match self.tier {
                MomentumTier::Fever => MomentumTier::Hot,
                MomentumTier::Hot => MomentumTier::Warming,
                MomentumTier::Warming => MomentumTier::Cold,
                MomentumTier::Cold => MomentumTier::Cold,
            };
            // The combo has timed out: the run's window goes with it, so a
            // formation that wakes up rebuilds from its new tier instead of
            // springing back on the first tick.
            self.window = RunWindow::default();
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

    /// Returns whether a promotion happened.
    fn try_promote(&mut self) -> bool {
        let old_tier = self.tier;
        let (row, new_tier) = match self.tier {
            MomentumTier::Cold => (0, MomentumTier::Warming),
            MomentumTier::Warming => (1, MomentumTier::Hot),
            MomentumTier::Hot => (2, MomentumTier::Fever),
            MomentumTier::Fever => return false,
        };
        let Some(threshold) = self.config.promote.get(row) else {
            return false;
        };
        if !self.window.clears(threshold) {
            return false;
        }
        self.tier = new_tier;
        self.last_transition = Some(Instant::now());
        // Promotion out of Cold resets the L6 intervention counter — the
        // formation has shown signs of life so the "stuck in Cold" alarm
        // restarts.
        self.cold_ticks = 0;
        tracing::info!(
            from = ?old_tier,
            to = ?new_tier,
            actions = self.window.actions,
            success_rate = self.window.success_rate(),
            duplicate_rate = self.window.duplicate_rate(),
            "momentum promoted"
        );
        true
    }

    /// Rate-based demotion, symmetric with promotion: once the window
    /// holds the current tier's `min_actions`, a success rate below that
    /// row's `min_success` or a duplicate rate above its `max_duplicate`
    /// drops one step and restarts the window.
    fn try_demote(&mut self) {
        let (row, lower) = match self.tier {
            MomentumTier::Cold => return,
            MomentumTier::Warming => (0, MomentumTier::Cold),
            MomentumTier::Hot => (1, MomentumTier::Warming),
            MomentumTier::Fever => (2, MomentumTier::Hot),
        };
        let Some(threshold) = self.config.promote.get(row) else {
            return;
        };
        if !self.window.fails(threshold) {
            return;
        }
        self.demote_to(lower, "cooperation rate fell below the tier's row");
    }

    /// One step down. The run restarts: the window is cleared.
    fn demote_to(&mut self, new_tier: MomentumTier, reason: &'static str) {
        let old_tier = self.tier;
        tracing::info!(
            from = ?old_tier,
            to = ?new_tier,
            actions = self.window.actions,
            success_rate = self.window.success_rate(),
            duplicate_rate = self.window.duplicate_rate(),
            reason,
            "momentum demoted"
        );
        self.tier = new_tier;
        self.last_transition = Some(Instant::now());
        self.window = RunWindow::default();
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
            MomentumEvent::TickSuccess { counts } => {
                self.record_successful_tick(counts);
                if counts.actions > 0 {
                    self.record_activity();
                }
            }
            MomentumEvent::TickIdle => {
                // Nobody acted: no counter change either way. The decay
                // clock keeps running because `last_activity` is untouched.
            }
            MomentumEvent::TickInterference { count } => {
                for _ in 0..*count {
                    self.record_interference();
                }
            }
            MomentumEvent::TickFailure { counts } => {
                self.record_failed_tick(counts);
                // A failing formation is active, not idle: decay measures
                // inactivity, and the AGT rule is only that idle time
                // cannot raise the score. Counters are unchanged by this.
                self.record_activity();
            }
            MomentumEvent::IntentChanged(_new_intent) => {
                // New intent, new run: what counted as cooperation before
                // may not now.
                self.consecutive_successes = 0;
                self.window = RunWindow::default();
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

    fn success(actions: u32, duplicates: u32) -> MomentumEvent {
        MomentumEvent::TickSuccess {
            counts: TickCounts {
                actions,
                successes: actions,
                duplicates,
                ..TickCounts::default()
            },
        }
    }

    fn clean(actions: u32) -> MomentumEvent {
        success(actions, 0)
    }

    #[test]
    fn test_promote_to_warming_on_action_rate_not_ticks() {
        let mut state = MomentumState::default();
        // One tick, three clean actions: 3 actions at 100% clears the row.
        state.apply_event(&clean(3));
        assert_eq!(state.tier, MomentumTier::Warming);
        assert_eq!(state.consecutive_successes, 1, "one tick, not three");
        assert_eq!(state.window.actions, 3);
        assert!(state.can_read_neighbor_state());
        assert!(state.can_chain());
        assert!(!state.can_write_environment());
    }

    #[test]
    fn test_promote_to_hot_needs_eight_actions_and_few_duplicates() {
        let mut state = MomentumState::default();
        state.apply_event(&clean(3)); // Warming
        state.apply_event(&clean(4)); // 7 actions: one short
        assert_eq!(state.tier, MomentumTier::Warming);
        state.apply_event(&success(2, 1)); // 9 actions, 1 duplicate: 0.11 <= 0.30
        assert_eq!(state.tier, MomentumTier::Hot);
        assert!(state.can_write_environment());
        assert!(state.can_synchronized_commit());
        assert!(!state.can_consensus());
    }

    #[test]
    fn test_promote_to_fever_needs_fifteen_actions_under_ten_percent_duplicates() {
        let mut state = MomentumState::default();
        state.apply_event(&clean(3)); // Warming
        state.apply_event(&clean(5)); // 8 actions: Hot
        state.apply_event(&success(4, 2)); // 12 actions, 2 duplicates
        state.apply_event(&clean(3)); // 15 actions but 0.133 duplicate > 0.10
        assert_eq!(state.tier, MomentumTier::Hot, "duplicate rate blocks Fever");
        state.apply_event(&clean(6)); // 21 actions, 0.095 duplicate
        assert_eq!(state.tier, MomentumTier::Fever);
        assert!(state.can_consensus());
        assert!(state.can_ai_orchestrate());
        assert!(state.can_recruit());
    }

    #[test]
    fn test_interference_resets_combo() {
        let mut state = MomentumState::default();
        for _ in 0..7 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Warming);
        state.record_interference();
        // The combo is broken: counter restarts from zero, the per-run
        // count is clear, the lifetime total is kept, the tier holds.
        assert_eq!(state.consecutive_successes, 0);
        assert_eq!(state.interference_count, 0);
        assert_eq!(state.interference_total, 1);
        assert_eq!(state.window.actions, 0, "the window is the run");
        assert_eq!(state.tier, MomentumTier::Warming);
        // Rebuild: eight clean successes reach Hot. A past interference
        // never blocks promotion.
        for _ in 0..8 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Hot);
    }

    #[test]
    fn test_duplicated_work_every_tick_never_reaches_hot() {
        let mut state = MomentumState::default();
        // Two members do the same thing every tick: 100% success, half the
        // actions duplicate. Cold → Warming tolerates it (max 1.00);
        // Warming → Hot does not (max 0.30), however long the run.
        for _ in 0..100 {
            state.apply_event(&success(2, 1));
            assert!(
                state.tier <= MomentumTier::Warming,
                "duplicate work must not reach Hot"
            );
        }
        assert_eq!(state.tier, MomentumTier::Warming);
        assert_eq!(state.window.actions, 200);
        assert_eq!(state.window.duplicate_actions, 100);
    }

    #[test]
    fn test_idle_ticks_never_promote() {
        let mut state = MomentumState::default();
        let before = state.last_activity;
        for _ in 0..100 {
            state.apply_event(&MomentumEvent::TickIdle);
        }
        assert_eq!(state.tier, MomentumTier::Cold);
        assert_eq!(state.consecutive_successes, 0);
        assert_eq!(
            state.last_activity, before,
            "idle must not refresh activity"
        );
    }

    #[test]
    fn test_forced_demotion_one_step_per_interval() {
        let interval = std::time::Duration::from_millis(15);
        let mut state = MomentumState {
            decay_interval: interval,
            ..MomentumState::default()
        };
        for _ in 0..15 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Fever);

        // Long idle: forced demotion is due, but never more than one step
        // per interval, and the tier never rises.
        state.last_activity = Instant::now() - std::time::Duration::from_secs(100);
        let mut previous = state.tier;
        for _ in 0..2 {
            std::thread::sleep(interval + std::time::Duration::from_millis(1));
            state.check_decay();
            let stepped = state.tier;
            assert!(stepped < previous, "exactly one forced step per interval");
            // Same interval again: the guard holds, no second step.
            state.check_decay();
            assert_eq!(state.tier, stepped, "forced demotion must not cascade");
            previous = stepped;
        }
        assert_eq!(state.tier, MomentumTier::Warming);
    }

    #[test]
    fn test_failed_ticks_demote_by_rate_not_by_one() {
        let mut state = MomentumState::default();
        for _ in 0..5 {
            state.record_success();
        }
        assert_eq!(state.tier, MomentumTier::Warming);
        state.last_activity = Instant::now() - std::time::Duration::from_secs(5);
        let failed = MomentumEvent::TickFailure {
            counts: TickCounts {
                actions: 1,
                ..TickCounts::default()
            },
        };
        state.apply_event(&failed);
        assert_eq!(state.consecutive_successes, 0);
        // One failed action in six: 0.83 still clears Warming's 0.80 row,
        // so the tier holds and the failure stays in the window.
        assert_eq!(state.tier, MomentumTier::Warming);
        assert_eq!(state.window.actions, 6);
        assert!(state.window.success_rate() < 0.9);
        // A second failure: 5/7 = 0.71 is under the row. Rate-based
        // demotion drops one step and restarts the run.
        state.apply_event(&failed);
        assert_eq!(state.tier, MomentumTier::Cold);
        assert_eq!(state.window.actions, 0);
        // Failure is activity: the decay clock is refreshed.
        assert!(state.last_activity.elapsed() < std::time::Duration::from_secs(1));
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
        assert_eq!(state.consecutive_successes, 0);
        assert_eq!(state.interference_total, 1);
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
