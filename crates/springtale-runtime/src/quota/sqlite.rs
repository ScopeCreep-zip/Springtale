//! SQLite-backed per-bot daily token quota.
//!
//! Mirrors the contract of [`springtale_ai::InMemoryTokenQuota`] but
//! routes counters through the daemon's `StorageBackend`. Daemon
//! restart no longer resets a bot's quota — the daily counter walks
//! the calendar with the wall clock.
//!
//! Day packing matches the in-memory backend (`year*1000 + ordinal`)
//! so the two impls are swap-compatible during boot.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use springtale_ai::error::AiError;
use springtale_ai::{QuotaCheck, TokenQuota};
use springtale_store::{AiTokenReserveOutcome, StorageBackend};

/// SQLite-backed quota. Wraps an `Arc<dyn StorageBackend>` so it
/// shares the daemon's connection pool.
pub struct SqliteTokenQuota {
    store: Arc<dyn StorageBackend>,
    /// Daily cap per bot. `None` records usage without enforcement —
    /// observability mode for installs that want metrics but no
    /// hard cap.
    daily_limit: Option<u64>,
    /// Clock override for tests. `None` in production uses
    /// `chrono::Utc::now`.
    #[cfg(test)]
    clock: Option<Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>>,
}

impl std::fmt::Debug for SqliteTokenQuota {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteTokenQuota")
            .field("daily_limit", &self.daily_limit)
            .finish_non_exhaustive()
    }
}

impl SqliteTokenQuota {
    /// Wrap a storage backend with a per-bot daily cap. Passing
    /// `None` for `daily_limit` enables observability mode — every
    /// call records usage but the quota never denies.
    pub fn new(store: Arc<dyn StorageBackend>, daily_limit: Option<u64>) -> Self {
        Self {
            store,
            daily_limit,
            #[cfg(test)]
            clock: None,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return (clock)();
        }
        Utc::now()
    }

    /// Pack a UTC timestamp into the YYYY*1000 + ordinal-day-of-year
    /// integer the schema uses for `day_ymd`. Stable across
    /// process restarts.
    fn day_key(now: DateTime<Utc>) -> u32 {
        (now.year() as u32).saturating_mul(1000) + now.ordinal()
    }
}

#[async_trait]
impl TokenQuota for SqliteTokenQuota {
    async fn check_and_reserve(
        &self,
        agent_id: &str,
        requested_tokens: u64,
    ) -> Result<QuotaCheck, AiError> {
        let day = Self::day_key(self.now());
        let outcome = self
            .store
            .ai_token_usage_reserve(agent_id, day, requested_tokens, self.daily_limit)
            .await
            .map_err(|e| AiError::QuotaBackend(format!("quota reserve: {e}")))?;
        match outcome {
            AiTokenReserveOutcome::Reserved { total_after } => {
                let remaining = match self.daily_limit {
                    Some(limit) => limit.saturating_sub(total_after),
                    None => u64::MAX,
                };
                Ok(QuotaCheck::Allowed { remaining })
            }
            AiTokenReserveOutcome::Denied { used, limit } => Ok(QuotaCheck::Denied { used, limit }),
        }
    }

    async fn commit(
        &self,
        agent_id: &str,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), AiError> {
        let day = Self::day_key(self.now());
        // Atomic backend call — runs under the backend's connection
        // lock so two concurrent commits for the same (agent, day)
        // can't race a stale `tokens_used` value into the write.
        self.store
            .ai_token_usage_commit(agent_id, day, prior_reservation, actual_tokens)
            .await
            .map_err(|e| AiError::QuotaBackend(format!("quota commit: {e}")))?;
        Ok(())
    }

    async fn usage(&self, agent_id: &str) -> Result<u64, AiError> {
        let day = Self::day_key(self.now());
        self.store
            .ai_token_usage_get(agent_id, day)
            .await
            .map_err(|e| AiError::QuotaBackend(format!("quota usage: {e}")))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use springtale_store::backend::InMemoryBackend;

    fn fixed_clock(dt: DateTime<Utc>) -> Arc<dyn Fn() -> DateTime<Utc> + Send + Sync> {
        Arc::new(move || dt)
    }

    fn quota(
        backend: Arc<InMemoryBackend>,
        limit: Option<u64>,
        clock: DateTime<Utc>,
    ) -> SqliteTokenQuota {
        let store: Arc<dyn StorageBackend> = backend;
        let mut q = SqliteTokenQuota::new(store, limit);
        q.clock = Some(fixed_clock(clock));
        q
    }

    #[tokio::test]
    async fn reserve_under_cap_allowed_then_denied() {
        let backend = Arc::new(InMemoryBackend::new());
        let q = quota(
            backend,
            Some(100),
            Utc.with_ymd_and_hms(2026, 6, 2, 0, 0, 0).unwrap(),
        );
        let a = q.check_and_reserve("bot-1", 60).await.unwrap();
        assert!(matches!(a, QuotaCheck::Allowed { remaining: 40 }));
        let b = q.check_and_reserve("bot-1", 50).await.unwrap();
        assert!(matches!(
            b,
            QuotaCheck::Denied {
                used: 60,
                limit: 100,
            }
        ));
    }

    #[tokio::test]
    async fn restart_preserves_counter() {
        let backend = Arc::new(InMemoryBackend::new());
        let day = Utc.with_ymd_and_hms(2026, 6, 2, 0, 0, 0).unwrap();
        {
            let q = quota(backend.clone(), Some(1_000), day);
            q.check_and_reserve("bot-1", 250).await.unwrap();
        }
        // "Restart" — drop the SqliteTokenQuota handle and build a
        // new one over the same backend. The counter must persist.
        let q2 = quota(backend, Some(1_000), day);
        assert_eq!(q2.usage("bot-1").await.unwrap(), 250);
        let next = q2.check_and_reserve("bot-1", 100).await.unwrap();
        assert!(matches!(next, QuotaCheck::Allowed { remaining: 650 }));
    }

    #[tokio::test]
    async fn commit_replaces_pessimistic_reservation() {
        let backend = Arc::new(InMemoryBackend::new());
        let q = quota(
            backend,
            Some(1_000),
            Utc.with_ymd_and_hms(2026, 6, 2, 0, 0, 0).unwrap(),
        );
        q.check_and_reserve("bot-1", 500).await.unwrap();
        q.commit("bot-1", 500, 120).await.unwrap();
        // 500 reserved, 120 actually used → counter should reflect 120.
        assert_eq!(q.usage("bot-1").await.unwrap(), 120);
    }
}
