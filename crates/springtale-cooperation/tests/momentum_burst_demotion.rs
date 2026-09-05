//! Momentum tier lifecycle integration test (Phase-7 audit Finding C).
//!
//! Walks the full FSM cold → warming → hot → fever via record_success,
//! then drives decay back down by aging `last_activity` into the past
//! and invoking `check_decay`. Validates the tier-gated capability set
//! at each transition + the L6 intervention `cold_ticks` counter.
//!
//! The unit tests in `src/momentum.rs` cover individual transitions in
//! isolation; this test exercises the full lifecycle in one process so
//! a regression that breaks any single transition fails here too.

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use springtale_cooperation::momentum::{MomentumState, MomentumTier};

/// Helper: forge an aged `last_activity` (and `last_transition`, which
/// gates forced demotion to one step per `decay_interval`) so
/// `check_decay` observes the configured idle interval without sleeping
/// in real wall-clock time.
fn age_activity(state: &mut MomentumState, age: Duration) {
    state.last_activity = Instant::now().checked_sub(age).unwrap_or(Instant::now());
    state.last_transition = state
        .last_transition
        .map(|t| t.checked_sub(age).unwrap_or(t));
}

#[test]
fn momentum_burst_promotes_then_decay_demotes() {
    let mut state = MomentumState {
        decay_interval: Duration::from_secs(60),
        ..MomentumState::default()
    };

    // Initial: Cold. No neighbor-state reads, no chaining.
    assert_eq!(state.tier, MomentumTier::Cold);
    assert!(!state.can_read_neighbor_state());
    assert!(!state.can_chain());
    assert!(!state.can_consensus());

    // ── Burst of activity: ramp through Warming → Hot → Fever ──
    // The thresholds (3 / 8 / 15) come from `src/momentum.rs` and
    // mirror COOPERATION.pdf §13's Cold → Warming → Hot → Fever
    // ladder. We assert the full step sequence so any threshold
    // tweak gets caught.
    for _ in 0..3 {
        state.record_success();
    }
    assert_eq!(state.tier, MomentumTier::Warming);
    assert!(state.can_read_neighbor_state());
    assert!(state.can_chain());
    assert!(!state.can_write_environment());

    for _ in 3..8 {
        state.record_success();
    }
    assert_eq!(state.tier, MomentumTier::Hot);
    assert!(state.can_write_environment());
    assert!(state.can_synchronized_commit());
    assert!(!state.can_consensus());

    for _ in 8..15 {
        state.record_success();
    }
    assert_eq!(state.tier, MomentumTier::Fever);
    assert!(state.can_consensus());
    assert!(state.can_ai_orchestrate());
    assert!(state.can_recruit());

    // Record real activity so subsequent decay logic isn't a no-op
    // (record_success doesn't refresh last_activity per the FSM docs).
    state.record_activity();

    // ── Quiet period: force-demote down the ladder by aging
    //    last_activity past `decay_interval * 3`. The FSM has two
    //    decay modes; mode 2 (forced demotion at 3× interval)
    //    fires when 0 successes remain but the tier is still
    //    elevated, which is the case after Fever-promote consumed
    //    every counter into the threshold. ──
    age_activity(&mut state, Duration::from_secs(60 * 4));
    state.check_decay();
    // After one decay pass we drop ONE rung (Fever → Hot in mode 2).
    assert_eq!(state.tier, MomentumTier::Hot);

    age_activity(&mut state, Duration::from_secs(60 * 4));
    state.check_decay();
    assert_eq!(state.tier, MomentumTier::Warming);

    age_activity(&mut state, Duration::from_secs(60 * 4));
    state.check_decay();
    assert_eq!(state.tier, MomentumTier::Cold);

    // ── Stuck-in-Cold L6 intervention signal ──
    // Once Cold, every check_decay should increment cold_ticks so the
    // intervention layer can decide when to EscalateToUser. We invoke
    // check_decay a few times and verify the counter rises.
    let baseline = state.cold_ticks;
    for _ in 0..5 {
        state.check_decay();
    }
    assert_eq!(state.cold_ticks, baseline + 5);

    // Promotion back out of Cold resets the cold_ticks counter so the
    // intervention alarm restarts.
    for _ in 0..3 {
        state.record_success();
    }
    assert_eq!(state.tier, MomentumTier::Warming);
    assert_eq!(state.cold_ticks, 0);
}
