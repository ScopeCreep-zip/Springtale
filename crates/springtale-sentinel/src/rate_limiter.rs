use std::collections::VecDeque;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Per-connector sliding window rate limiter.
///
/// Tracks action timestamps in a 1-minute window. When the count
/// exceeds the limit, returns a throttle duration.
pub struct RateLimiter {
    windows: DashMap<String, SlidingWindow>,
    limit: u32,
    window_duration: Duration,
}

struct SlidingWindow {
    timestamps: VecDeque<Instant>,
}

impl RateLimiter {
    /// Create a rate limiter with the given per-connector limit per window.
    pub fn new(limit_per_minute: u32) -> Self {
        Self {
            windows: DashMap::new(),
            limit: limit_per_minute,
            window_duration: Duration::from_secs(60),
        }
    }

    /// Check if an action for the given connector is within the rate
    /// limit. Returns `None` if allowed, or `Some(duration)` to
    /// throttle.
    ///
    /// Uses the limiter's configured baseline (`limit` /
    /// `window_duration`). For momentum-aware throttling, prefer
    /// [`Self::check_at_tier`] — pre-Phase-0 callers can stay on this
    /// path until they thread cooperation context.
    pub fn check(&self, connector_name: &str) -> Option<Duration> {
        self.check_with_budget(connector_name, self.limit, self.window_duration)
    }

    /// Tier-scoped check. The caller's
    /// [`crate::ThrottleTier::rate_budget`] supplies the effective
    /// `(limit, window)` pair, overriding the limiter's configured
    /// baseline for this call only. Used by the runtime dispatcher
    /// so a Fever-tier swarm isn't throttled to the same baseline as
    /// a Cold solo observer.
    pub fn check_at_tier(
        &self,
        connector_name: &str,
        tier: crate::ThrottleTier,
    ) -> Option<Duration> {
        let (limit, window) = tier.rate_budget();
        self.check_with_budget(connector_name, limit, window)
    }

    fn check_with_budget(
        &self,
        connector_name: &str,
        limit: u32,
        window_duration: Duration,
    ) -> Option<Duration> {
        let now = Instant::now();
        let mut window = self
            .windows
            .entry(connector_name.to_owned())
            .or_insert_with(|| SlidingWindow {
                timestamps: VecDeque::new(),
            });

        // Remove expired entries
        let cutoff = now - window_duration;
        while window.timestamps.front().is_some_and(|t| *t < cutoff) {
            window.timestamps.pop_front();
        }

        if window.timestamps.len() >= limit as usize {
            // Calculate how long until the oldest entry expires
            if let Some(oldest) = window.timestamps.front() {
                let wait = window_duration - (now - *oldest);
                return Some(wait);
            }
        }

        // Record this action
        window.timestamps.push_back(now);
        None
    }

    /// Reset the rate limiter for a specific connector.
    pub fn reset(&self, connector_name: &str) {
        self.windows.remove(connector_name);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_under_limit_allows() {
        let limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check("test").is_none());
        }
    }

    #[test]
    fn test_at_limit_throttles() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.check("test").is_none());
        assert!(limiter.check("test").is_none());
        assert!(limiter.check("test").is_none());
        // 4th call should throttle
        assert!(limiter.check("test").is_some());
    }

    #[test]
    fn test_per_connector_isolation() {
        let limiter = RateLimiter::new(2);
        assert!(limiter.check("a").is_none());
        assert!(limiter.check("a").is_none());
        assert!(limiter.check("a").is_some()); // a is throttled
        assert!(limiter.check("b").is_none()); // b is independent
    }

    #[test]
    fn test_reset_clears_window() {
        let limiter = RateLimiter::new(1);
        assert!(limiter.check("test").is_none());
        assert!(limiter.check("test").is_some());
        limiter.reset("test");
        assert!(limiter.check("test").is_none()); // allowed again
    }
}
