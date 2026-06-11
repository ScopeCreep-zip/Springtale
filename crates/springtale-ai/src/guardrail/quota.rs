//! Per-bot daily token quota.
//!
//! OWASP LLM10 (Unbounded Consumption) mitigation: cap the number of
//! tokens a single bot can spend per UTC day. Defends against:
//!
//! * A compromised connector pumping attacker-controlled content into
//!   an AI step that bills per-token (OpenAI / Anthropic).
//! * A misconfigured loop that runs an AI step repeatedly without
//!   any progress check.
//! * Sudden cost spikes that obscure earlier compromises.
//!
//! The trait is the stable contract — backends can persist into the
//! daemon's SQLite store, into an external metrics service, or sit
//! purely in-process. [`InMemoryTokenQuota`] is the in-process
//! backend that ships today. A SQLite-backed backend in
//! `springtale-store` is the planned next step; switching backends
//! is a one-line change at the wrapper construction site.

use std::collections::HashMap;
#[cfg(test)]
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use tokio::sync::Mutex;

use crate::error::AiError;

/// Outcome of a pre-call quota check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaCheck {
    /// The call may proceed; estimated `tokens` reserved.
    Allowed { remaining: u64 },
    /// The call is denied; the quota is already exhausted.
    Denied { used: u64, limit: u64 },
}

/// Pluggable per-bot daily token quota backend.
///
/// Storage detail (in-process map, SQLite, Redis, etc.) is the impl's
/// concern. The wrapper calls `check_and_reserve` BEFORE the AI
/// request and `commit` AFTER the response returns. `commit` accepts
/// the actual tokens consumed — usually different from the
/// pre-reserve estimate (`AiOptions::max_tokens`).
#[async_trait]
pub trait TokenQuota: Send + Sync + 'static {
    /// Ask the backend whether `agent_id` can spend up to
    /// `requested_tokens` today. The backend should reserve the
    /// pessimistic upper bound so concurrent calls can't race past
    /// the cap.
    async fn check_and_reserve(
        &self,
        agent_id: &str,
        requested_tokens: u64,
    ) -> Result<QuotaCheck, AiError>;

    /// Finalise the reservation with the actual tokens consumed.
    /// Backends release the over-reservation (`prior_reservation -
    /// actual_tokens`) so the day's running total reflects real
    /// usage, not the pessimistic pre-flight estimate. Called on
    /// success AND on failure — the wrapper passes `actual_tokens =
    /// 0` on transport errors so the reservation rolls back fully.
    async fn commit(
        &self,
        agent_id: &str,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), AiError>;

    /// Current quota usage for `agent_id` today. Used by the admin
    /// API to surface quota state without forcing a fake call.
    async fn usage(&self, agent_id: &str) -> Result<u64, AiError>;
}

/// In-process daily quota — one process, one daemon session.
///
/// Quota state lives in memory; daemon restart resets the counters.
/// Acceptable for the threat model because:
///
/// 1. OWASP LLM10's primary concern is intra-session unbounded
///    consumption (one request burst that goes wild), not multi-day
///    drift.
/// 2. The daemon is typically long-running on a user's machine; a
///    daemon restart is a deliberate user action.
/// 3. The trait shape allows swapping to a SQLite-backed backend
///    without changing any caller code.
pub struct InMemoryTokenQuota {
    /// `(agent_id, day_ymd)` → tokens used today.
    state: Mutex<HashMap<QuotaKey, u64>>,
    /// Daily cap per bot. `None` means "unlimited" — the quota check
    /// always passes but usage is still recorded for the admin API.
    daily_limit: Option<u64>,
    /// Clock override for tests. `None` in production uses `Utc::now`.
    #[cfg(test)]
    clock: Option<Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>>,
}

impl std::fmt::Debug for InMemoryTokenQuota {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryTokenQuota")
            .field("daily_limit", &self.daily_limit)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct QuotaKey {
    agent_id: String,
    day_ymd: u32,
}

impl InMemoryTokenQuota {
    /// Build a quota with the given daily limit. `None` records usage
    /// but never denies — useful when the user wants observability
    /// without enforcement.
    pub fn new(daily_limit: Option<u64>) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            daily_limit,
            #[cfg(test)]
            clock: None,
        }
    }

    /// Pack a `DateTime<Utc>` into a sortable u32 day-of-year key.
    /// (Year * 1000 + day-of-year) — collision-free for any year >0,
    /// and lets the map shed yesterday's keys lazily without
    /// requiring an explicit per-day reset task.
    fn day_key(&self, now: DateTime<Utc>) -> u32 {
        (now.year() as u32).saturating_mul(1000) + now.ordinal()
    }

    fn now(&self) -> DateTime<Utc> {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return (clock)();
        }
        Utc::now()
    }
}

#[async_trait]
impl TokenQuota for InMemoryTokenQuota {
    async fn check_and_reserve(
        &self,
        agent_id: &str,
        requested_tokens: u64,
    ) -> Result<QuotaCheck, AiError> {
        let day_ymd = self.day_key(self.now());
        let key = QuotaKey {
            agent_id: agent_id.to_owned(),
            day_ymd,
        };
        let mut state = self.state.lock().await;
        let used = *state.get(&key).unwrap_or(&0);
        let new_total = used.saturating_add(requested_tokens);
        if let Some(limit) = self.daily_limit
            && new_total > limit
        {
            return Ok(QuotaCheck::Denied { used, limit });
        }
        // Optimistic reserve — the eventual `commit` finalises with
        // the actual tokens used.
        state.insert(key, new_total);
        let remaining = match self.daily_limit {
            Some(limit) => limit.saturating_sub(new_total),
            None => u64::MAX,
        };
        Ok(QuotaCheck::Allowed { remaining })
    }

    async fn commit(
        &self,
        agent_id: &str,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), AiError> {
        let day_ymd = self.day_key(self.now());
        let key = QuotaKey {
            agent_id: agent_id.to_owned(),
            day_ymd,
        };
        let mut state = self.state.lock().await;
        // Replace the pessimistic pre-reservation with the actual
        // count: subtract what we reserved, add what was actually
        // used. Saturating arithmetic guards against an out-of-order
        // commit that arrives after another reset.
        if let Some(entry) = state.get_mut(&key) {
            *entry = (*entry)
                .saturating_sub(prior_reservation)
                .saturating_add(actual_tokens);
        } else {
            // Day rolled over between reserve and commit — the new
            // day's row starts at the actual usage we observed.
            state.insert(key, actual_tokens);
        }
        Ok(())
    }

    async fn usage(&self, agent_id: &str) -> Result<u64, AiError> {
        let day_ymd = self.day_key(self.now());
        let key = QuotaKey {
            agent_id: agent_id.to_owned(),
            day_ymd,
        };
        let state = self.state.lock().await;
        Ok(*state.get(&key).unwrap_or(&0))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_clock(dt: DateTime<Utc>) -> Arc<dyn Fn() -> DateTime<Utc> + Send + Sync> {
        Arc::new(move || dt)
    }

    fn new_with_clock(
        limit: Option<u64>,
        clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
    ) -> InMemoryTokenQuota {
        let mut q = InMemoryTokenQuota::new(limit);
        q.clock = Some(clock);
        q
    }

    #[tokio::test]
    async fn unlimited_quota_always_allows() {
        let q = InMemoryTokenQuota::new(None);
        let r = q.check_and_reserve("bot-1", 1_000_000).await.unwrap();
        assert!(matches!(r, QuotaCheck::Allowed { .. }));
    }

    #[tokio::test]
    async fn limited_quota_denies_at_cap() {
        let q = InMemoryTokenQuota::new(Some(100));
        let r1 = q.check_and_reserve("bot-1", 80).await.unwrap();
        assert!(matches!(r1, QuotaCheck::Allowed { remaining: 20 }));
        let r2 = q.check_and_reserve("bot-1", 30).await.unwrap();
        assert!(matches!(
            r2,
            QuotaCheck::Denied {
                used: 80,
                limit: 100
            }
        ));
    }

    #[tokio::test]
    async fn usage_returns_zero_for_unknown_bot() {
        let q = InMemoryTokenQuota::new(Some(100));
        assert_eq!(q.usage("nobody").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn day_rollover_resets_counter() {
        // Reserve under day A, then advance the clock to day B and
        // confirm the counter starts fresh.
        let day_a = Utc.with_ymd_and_hms(2026, 5, 30, 12, 0, 0).unwrap();
        let day_b = Utc.with_ymd_and_hms(2026, 5, 31, 12, 0, 0).unwrap();
        let q = new_with_clock(Some(50), fixed_clock(day_a));
        q.check_and_reserve("bot-1", 40).await.unwrap();
        assert_eq!(q.usage("bot-1").await.unwrap(), 40);

        // Swap clock to next day.
        let mut q = q;
        q.clock = Some(fixed_clock(day_b));
        assert_eq!(q.usage("bot-1").await.unwrap(), 0);
    }
}
