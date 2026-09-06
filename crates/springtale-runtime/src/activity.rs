//! When this runtime last did something a person asked for.
//!
//! The auto-lock timer (plan 6.10) fires after `auto_lock_secs` with "no
//! authenticated request and no chat message". The two halves of that
//! sentence live on opposite sides of the crate boundary — API requests
//! are the daemon's business, inbound chat is the runtime's — so the
//! shared stamp lives here, on `RuntimeState`, and both sides touch it.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A shared "last active at" stamp, in whole seconds since the epoch.
///
/// Cheap to clone; every clone reads and writes the same instant.
#[derive(Clone)]
pub struct ActivityClock {
    at: Arc<AtomicI64>,
}

impl Default for ActivityClock {
    fn default() -> Self {
        Self {
            at: Arc::new(AtomicI64::new(now_secs())),
        }
    }
}

impl ActivityClock {
    /// A clock stamped with the current instant.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record activity now.
    pub fn touch(&self) {
        self.at.store(now_secs(), Ordering::Release);
    }

    /// Seconds since the last [`touch`](Self::touch). Saturates at zero
    /// so a backwards clock step cannot report a negative idle time (and
    /// so cannot delay an auto-lock).
    pub fn idle_secs(&self) -> u64 {
        let last = self.at.load(Ordering::Acquire);
        now_secs().saturating_sub(last).max(0).unsigned_abs()
    }
}

/// Seconds since the Unix epoch, or 0 if the system clock is before it.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_clock_is_not_idle() {
        assert_eq!(ActivityClock::new().idle_secs(), 0);
    }

    #[test]
    fn test_touch_resets_idle() {
        let clock = ActivityClock::new();
        clock.at.store(now_secs() - 120, Ordering::Release);
        assert!(clock.idle_secs() >= 120);
        clock.touch();
        assert_eq!(clock.idle_secs(), 0);
    }

    #[test]
    fn test_clones_share_one_stamp() {
        let clock = ActivityClock::new();
        let other = clock.clone();
        clock.at.store(now_secs() - 60, Ordering::Release);
        assert!(other.idle_secs() >= 60);
        other.touch();
        assert_eq!(clock.idle_secs(), 0);
    }
}
