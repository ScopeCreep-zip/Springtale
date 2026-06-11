//! Process-local refusal-rate metric.
//!
//! Counts AI calls + the subset that were blocked by the input
//! sanitiser (sanitisation policy = `Block` → `AiError::SanitizationBlocked`).
//! Surfaced via the admin API so OWASP LLM07 (System Prompt
//! Leakage) and LLM01 (Prompt Injection) signals are observable
//! without parsing logs.
//!
//! This is the AI-layer refusal rate — distinct from provider-side
//! refusals (the model declined the prompt), which surface as
//! `InferenceFailed` and are not counted here.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic refusal counter. Cheap to clone — internal state lives
/// behind an `Arc` so multiple `GuardrailAdapter` instances can share
/// one counter (e.g. one counter per `RuntimeState`, shared by every
/// hot-swapped adapter).
#[derive(Debug, Clone, Default)]
pub struct RefusalCounter {
    inner: Arc<RefusalCounterInner>,
}

#[derive(Debug, Default)]
struct RefusalCounterInner {
    total_calls: AtomicU64,
    total_refusals: AtomicU64,
}

impl RefusalCounter {
    /// Build a new counter starting at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one AI call attempt.
    pub fn record_call(&self) {
        self.inner.total_calls.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one sanitiser block.
    pub fn record_refusal(&self) {
        self.inner.total_refusals.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot the current counts.
    pub fn snapshot(&self) -> RefusalStats {
        RefusalStats {
            total_calls: self.inner.total_calls.load(Ordering::Relaxed),
            total_refusals: self.inner.total_refusals.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of refusal counts at a moment in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefusalStats {
    /// Total AI call attempts (whether the request reached the
    /// provider or was blocked at the sanitiser).
    pub total_calls: u64,
    /// Subset of `total_calls` that the sanitiser blocked. Refusal
    /// rate = `total_refusals / total_calls`.
    pub total_refusals: u64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn fresh_counter_is_zero() {
        let c = RefusalCounter::new();
        let s = c.snapshot();
        assert_eq!(s.total_calls, 0);
        assert_eq!(s.total_refusals, 0);
    }

    #[test]
    fn records_calls_and_refusals_independently() {
        let c = RefusalCounter::new();
        c.record_call();
        c.record_call();
        c.record_call();
        c.record_refusal();
        let s = c.snapshot();
        assert_eq!(s.total_calls, 3);
        assert_eq!(s.total_refusals, 1);
    }

    #[test]
    fn clones_share_state() {
        let c1 = RefusalCounter::new();
        let c2 = c1.clone();
        c1.record_call();
        c2.record_refusal();
        assert_eq!(c1.snapshot(), c2.snapshot());
        assert_eq!(c1.snapshot().total_calls, 1);
        assert_eq!(c1.snapshot().total_refusals, 1);
    }
}
