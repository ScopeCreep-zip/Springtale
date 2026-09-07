//! The auto-lock countdown, as a pure state machine.
//!
//! `crate::autolock::AutoLockHandle` is the OS-facing half: it turns
//! [`AutoLockTimer::countdown`] into a `tokio::time::sleep` and zeroes the
//! vault when that sleep wins the `select!`. The policy itself — how long
//! to wait, what "disabled" means, when idle time has crossed the
//! threshold — lives here, where it can be driven without sleeping.
//!
//! Time is supplied by the caller as a monotonic millisecond count rather
//! than read from a clock, so a test can step a whole afternoon of idleness
//! in a microsecond.

use std::time::Duration;

/// The persisted config counts minutes; every duration here is seconds.
const SECS_PER_MINUTE: u64 = 60;

/// Where the countdown stands at a given moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoLockState {
    /// `auto_lock_minutes == 0` — the survivor turned auto-lock off. No
    /// timer is armed and idleness never locks the vault.
    Disabled,
    /// Counting down. `remaining` is the time left before the vault locks
    /// if no further activity arrives.
    Counting {
        /// Time left until the threshold is crossed.
        remaining: Duration,
    },
    /// Idle time has reached the configured threshold — lock the vault.
    Locked,
}

/// Auto-lock timer state: a threshold plus the moment activity last reset it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoLockTimer {
    /// `None` when auto-lock is disabled.
    timeout: Option<Duration>,
    /// Monotonic timestamp (ms) of the activity that last restarted the
    /// countdown. Starts at zero — the first `record_activity` moves it.
    last_activity_ms: u64,
}

impl AutoLockTimer {
    /// Build a timer for a configured `auto_lock_minutes` value.
    ///
    /// Zero means disabled, which is the one value that must never arm a
    /// timer: a survivor who turned auto-lock off did so deliberately.
    #[must_use]
    pub fn new(timeout_minutes: u32) -> Self {
        let timeout = if timeout_minutes == 0 {
            None
        } else {
            Some(Duration::from_secs(
                u64::from(timeout_minutes) * SECS_PER_MINUTE,
            ))
        };
        Self {
            timeout,
            last_activity_ms: 0,
        }
    }

    /// How long a freshly-reset timer should wait before locking, or `None`
    /// when auto-lock is disabled and no timer should be armed at all.
    ///
    /// This is the production entry point — `AutoLockHandle::reset` sleeps
    /// for exactly this duration.
    #[must_use]
    pub fn countdown(&self) -> Option<Duration> {
        self.timeout
    }

    /// Whether a timer is armed at all.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.timeout.is_some()
    }

    /// Record user activity: the countdown restarts from `now_ms`.
    pub fn record_activity(&mut self, now_ms: u64) {
        self.last_activity_ms = now_ms;
    }

    /// Idle time accumulated at `now_ms`.
    ///
    /// Saturating: a clock that hands back an earlier instant reads as zero
    /// idle rather than wrapping to ~584 million years and locking instantly.
    #[must_use]
    pub fn idle_for(&self, now_ms: u64) -> Duration {
        Duration::from_millis(now_ms.saturating_sub(self.last_activity_ms))
    }

    /// The same policy the spawned timer enforces, expressed as a query so
    /// it can be asserted directly: disabled never locks, accumulated idle
    /// time at or past the threshold locks, anything short of it counts down.
    #[must_use]
    pub fn poll(&self, now_ms: u64) -> AutoLockState {
        let Some(timeout) = self.timeout else {
            return AutoLockState::Disabled;
        };
        match timeout.checked_sub(self.idle_for(now_ms)) {
            Some(remaining) if !remaining.is_zero() => AutoLockState::Counting { remaining },
            // Exactly at the threshold counts as crossed — the same moment
            // `tokio::time::sleep(timeout)` fires.
            _ => AutoLockState::Locked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AutoLockState, AutoLockTimer};
    use std::time::Duration;

    const MINUTE_MS: u64 = 60_000;

    #[test]
    fn test_countdown_five_minutes_is_three_hundred_seconds() {
        assert_eq!(
            AutoLockTimer::new(5).countdown(),
            Some(Duration::from_secs(300))
        );
    }

    #[test]
    fn test_countdown_zero_minutes_is_disabled() {
        let timer = AutoLockTimer::new(0);
        assert_eq!(timer.countdown(), None);
        assert!(!timer.is_enabled());
    }

    #[test]
    fn test_poll_zero_timeout_never_locks() {
        let timer = AutoLockTimer::new(0);
        assert_eq!(timer.poll(0), AutoLockState::Disabled);
        // A week of idleness still does not lock a disabled timer.
        assert_eq!(timer.poll(7 * 24 * 60 * MINUTE_MS), AutoLockState::Disabled);
    }

    #[test]
    fn test_poll_idle_accumulates_toward_the_threshold() {
        let timer = AutoLockTimer::new(5);
        assert_eq!(
            timer.poll(MINUTE_MS),
            AutoLockState::Counting {
                remaining: Duration::from_secs(240)
            }
        );
        assert_eq!(
            timer.poll(4 * MINUTE_MS),
            AutoLockState::Counting {
                remaining: Duration::from_secs(60)
            }
        );
    }

    #[test]
    fn test_idle_for_measures_since_last_activity() {
        let mut timer = AutoLockTimer::new(5);
        timer.record_activity(2 * MINUTE_MS);
        assert_eq!(timer.idle_for(3 * MINUTE_MS), Duration::from_secs(60));
    }

    #[test]
    fn test_idle_for_backwards_clock_reads_as_zero() {
        let mut timer = AutoLockTimer::new(5);
        timer.record_activity(10 * MINUTE_MS);
        assert_eq!(timer.idle_for(MINUTE_MS), Duration::ZERO);
    }

    #[test]
    fn test_poll_activity_resets_the_countdown() {
        let mut timer = AutoLockTimer::new(5);
        // Four minutes idle — one minute left.
        assert_eq!(
            timer.poll(4 * MINUTE_MS),
            AutoLockState::Counting {
                remaining: Duration::from_secs(60)
            }
        );
        // The survivor touches the app: the full five minutes are back.
        timer.record_activity(4 * MINUTE_MS);
        assert_eq!(
            timer.poll(4 * MINUTE_MS),
            AutoLockState::Counting {
                remaining: Duration::from_secs(300)
            }
        );
        // ...and what would have been the original deadline no longer locks.
        assert_eq!(
            timer.poll(5 * MINUTE_MS),
            AutoLockState::Counting {
                remaining: Duration::from_secs(240)
            }
        );
    }

    #[test]
    fn test_poll_threshold_exactly_reached_locks() {
        let timer = AutoLockTimer::new(5);
        assert_eq!(timer.poll(5 * MINUTE_MS), AutoLockState::Locked);
    }

    #[test]
    fn test_poll_past_threshold_stays_locked() {
        let timer = AutoLockTimer::new(1);
        assert_eq!(timer.poll(90 * 1_000), AutoLockState::Locked);
    }

    #[test]
    fn test_poll_after_reset_locks_one_threshold_later() {
        let mut timer = AutoLockTimer::new(5);
        timer.record_activity(3 * MINUTE_MS);
        assert_eq!(
            timer.poll(7 * MINUTE_MS),
            AutoLockState::Counting {
                remaining: Duration::from_secs(60)
            }
        );
        assert_eq!(timer.poll(8 * MINUTE_MS), AutoLockState::Locked);
    }
}
