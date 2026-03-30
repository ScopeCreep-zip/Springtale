use std::time::Duration;

/// Configuration for exponential backoff with jitter.
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Base delay before first retry.
    pub base_delay: Duration,

    /// Maximum delay (caps exponential growth).
    pub max_delay: Duration,

    /// Maximum number of retry attempts. 0 = no retries.
    pub max_attempts: u32,

    /// Jitter factor: ±10% by default (0.1).
    pub jitter_factor: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(300), // 5 minutes
            max_attempts: 5,
            jitter_factor: 0.1,
        }
    }
}

/// Tracks retry state for a single operation.
#[derive(Debug)]
pub struct RetryState {
    config: BackoffConfig,
    attempts: u32,
}

impl RetryState {
    /// Create a new retry state with the given config.
    pub fn new(config: BackoffConfig) -> Self {
        Self {
            config,
            attempts: 0,
        }
    }

    /// Record a failed attempt and return the delay before the next retry.
    ///
    /// Returns `None` if max attempts exceeded.
    pub fn next_delay(&mut self) -> Option<Duration> {
        if self.attempts >= self.config.max_attempts {
            return None;
        }

        let base = self.config.base_delay.as_secs_f64() * 2.0_f64.powi(self.attempts as i32);
        let capped = base.min(self.config.max_delay.as_secs_f64());

        // Apply jitter: ±jitter_factor
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let jitter_range = capped * self.config.jitter_factor;
        let jitter = rng.gen_range(-jitter_range..=jitter_range);
        let delay = (capped + jitter).max(0.0);

        self.attempts += 1;
        Some(Duration::from_secs_f64(delay))
    }

    /// Reset the retry state (e.g., after a successful operation).
    pub fn reset(&mut self) {
        self.attempts = 0;
    }

    /// Current attempt count.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Whether retries are exhausted.
    pub fn exhausted(&self) -> bool {
        self.attempts >= self.config.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BackoffConfig::default();
        assert_eq!(config.base_delay, Duration::from_secs(1));
        assert_eq!(config.max_attempts, 5);
    }

    #[test]
    fn test_exponential_growth() {
        let config = BackoffConfig {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(3600),
            max_attempts: 5,
            jitter_factor: 0.0, // no jitter for deterministic test
        };
        let mut state = RetryState::new(config);

        // Without jitter: 1s, 2s, 4s, 8s, 16s
        let d0 = state.next_delay().map(|d| d.as_secs());
        let d1 = state.next_delay().map(|d| d.as_secs());
        let d2 = state.next_delay().map(|d| d.as_secs());
        let d3 = state.next_delay().map(|d| d.as_secs());
        let d4 = state.next_delay().map(|d| d.as_secs());
        let d5 = state.next_delay(); // should be None (exhausted)

        assert_eq!(d0, Some(1));
        assert_eq!(d1, Some(2));
        assert_eq!(d2, Some(4));
        assert_eq!(d3, Some(8));
        assert_eq!(d4, Some(16));
        assert!(d5.is_none());
    }

    #[test]
    fn test_max_delay_cap() {
        let config = BackoffConfig {
            base_delay: Duration::from_secs(100),
            max_delay: Duration::from_secs(200),
            max_attempts: 3,
            jitter_factor: 0.0,
        };
        let mut state = RetryState::new(config);

        let d0 = state.next_delay().map(|d| d.as_secs());
        let d1 = state.next_delay().map(|d| d.as_secs());

        assert_eq!(d0, Some(100));
        assert_eq!(d1, Some(200)); // capped, not 200
    }

    #[test]
    fn test_jitter_within_range() {
        let config = BackoffConfig {
            base_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(100),
            max_attempts: 10,
            jitter_factor: 0.1, // ±10%
        };
        let mut state = RetryState::new(config);

        // First retry: base is 10s, jitter ±1s → should be between 9 and 11
        let delay = state.next_delay().map(|d| d.as_secs_f64());
        assert!(delay.is_some());
        let d = delay.unwrap_or(0.0);
        assert!(
            d >= 9.0 && d <= 11.0,
            "delay {d} not in expected range 9-11"
        );
    }

    #[test]
    fn test_reset() {
        let config = BackoffConfig {
            max_attempts: 2,
            jitter_factor: 0.0,
            ..Default::default()
        };
        let mut state = RetryState::new(config);

        state.next_delay();
        state.next_delay();
        assert!(state.exhausted());

        state.reset();
        assert!(!state.exhausted());
        assert_eq!(state.attempts(), 0);
        assert!(state.next_delay().is_some());
    }

    #[test]
    fn test_zero_max_attempts() {
        let config = BackoffConfig {
            max_attempts: 0,
            ..Default::default()
        };
        let mut state = RetryState::new(config);
        assert!(state.next_delay().is_none());
        assert!(state.exhausted());
    }
}
