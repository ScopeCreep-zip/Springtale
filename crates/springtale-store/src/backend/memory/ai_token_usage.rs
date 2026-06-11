//! In-memory AI token usage (Phase-7 audit Finding D).
//!
//! Mirrors the SQLite path's semantics for the in-memory backend:
//! UPSERT-style get / set / reserve over a `HashMap<(agent_id,
//! day_ymd), u64>`. Lets tests exercise the SqliteTokenQuota's
//! contract without touching disk.

use crate::backend::AiTokenReserveOutcome;
use crate::error::StoreError;

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn ai_token_usage_get_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
    ) -> Result<u64, StoreError> {
        let map = self.ai_token_usage.read().await;
        Ok(*map.get(&(agent_id.to_owned(), day_ymd)).unwrap_or(&0))
    }

    pub(super) async fn ai_token_usage_set_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
        tokens_used: u64,
    ) -> Result<(), StoreError> {
        let mut map = self.ai_token_usage.write().await;
        map.insert((agent_id.to_owned(), day_ymd), tokens_used);
        Ok(())
    }

    /// Atomic commit adjustment — mirrors the SQLite path's
    /// semantics under the in-memory write lock. Replaces the
    /// pessimistic reservation with actual usage in one critical
    /// section so concurrent commits can't lose updates.
    pub(super) async fn ai_token_usage_commit_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
        prior_reservation: u64,
        actual_tokens: u64,
    ) -> Result<(), StoreError> {
        let mut map = self.ai_token_usage.write().await;
        let key = (agent_id.to_owned(), day_ymd);
        let used = *map.get(&key).unwrap_or(&0);
        let adjusted = used
            .saturating_sub(prior_reservation)
            .saturating_add(actual_tokens);
        map.insert(key, adjusted);
        Ok(())
    }

    pub(super) async fn ai_token_usage_reserve_impl(
        &self,
        agent_id: &str,
        day_ymd: u32,
        requested: u64,
        limit: Option<u64>,
    ) -> Result<AiTokenReserveOutcome, StoreError> {
        let mut map = self.ai_token_usage.write().await;
        let key = (agent_id.to_owned(), day_ymd);
        let used = *map.get(&key).unwrap_or(&0);
        let new_total = used.saturating_add(requested);
        if let Some(cap) = limit
            && new_total > cap
        {
            return Ok(AiTokenReserveOutcome::Denied { used, limit: cap });
        }
        map.insert(key, new_total);
        Ok(AiTokenReserveOutcome::Reserved {
            total_after: new_total,
        })
    }
}
