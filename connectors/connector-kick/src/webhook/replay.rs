//! Replay protection for Kick webhooks (plan 5.2, finding 116).
//!
//! Kick documents `Kick-Event-Message-Id` as an idempotent key and
//! `Kick-Event-Message-Timestamp` as an RFC 3339 send time. Both checks
//! run AFTER signature verification so an unsigned request can never
//! poison the seen-id cache.
//!
//! State is held in-memory on the connector: the `Connector` trait hands
//! `verify_webhook` no storage handle, and the connector crate cannot
//! depend on `springtale-runtime` (dependency direction), so the
//! runtime's `dedupe` store is not reachable from here.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::KickError;

/// Maximum absolute skew between the event timestamp and now.
pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 5 * 60;

/// How long a message id is remembered after first sight.
pub const MESSAGE_ID_TTL: Duration = Duration::from_secs(60 * 60);

/// Reject a `Kick-Event-Message-Timestamp` that is unparseable or more
/// than [`MAX_TIMESTAMP_SKEW_SECS`] away from `now` in either direction.
pub fn check_timestamp(
    timestamp: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), KickError> {
    let sent = chrono::DateTime::parse_from_rfc3339(timestamp)
        .map_err(|e| KickError::RequestFailed(format!("invalid webhook timestamp: {e}")))?
        .with_timezone(&chrono::Utc);
    let skew_secs = now.signed_duration_since(sent).num_seconds();
    if skew_secs.abs() > MAX_TIMESTAMP_SKEW_SECS {
        return Err(KickError::RequestFailed(format!(
            "webhook timestamp outside the {MAX_TIMESTAMP_SKEW_SECS}s replay window"
        )));
    }
    Ok(())
}

/// Seen message ids with their first-sight instant, pruned on insert.
#[derive(Debug, Default)]
pub struct ReplayCache {
    seen: HashMap<String, Instant>,
}

impl ReplayCache {
    /// Record `message_id` at `now`; reject it if it was already seen
    /// within [`MESSAGE_ID_TTL`]. Expired entries are dropped first.
    pub fn check_and_record(&mut self, message_id: &str, now: Instant) -> Result<(), KickError> {
        self.seen
            .retain(|_, first_seen| now.duration_since(*first_seen) < MESSAGE_ID_TTL);
        if self.seen.contains_key(message_id) {
            return Err(KickError::RequestFailed(
                "webhook message id already seen (replay)".to_owned(),
            ));
        }
        self.seen.insert(message_id.to_owned(), now);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_check_timestamp_stale_rejected() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-04T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(check_timestamp("2026-09-04T11:56:00Z", now).is_ok());
        assert!(check_timestamp("2026-09-04T11:54:59Z", now).is_err());
        assert!(check_timestamp("not-a-timestamp", now).is_err());
    }

    #[test]
    fn test_replay_cache_repeated_id_rejected() {
        let mut cache = ReplayCache::default();
        let now = Instant::now();
        assert!(cache.check_and_record("msg-1", now).is_ok());
        assert!(cache.check_and_record("msg-1", now).is_err());
        assert!(cache.check_and_record("msg-2", now).is_ok());
        // Once the TTL has elapsed the id is forgotten and accepted again.
        let later = now + MESSAGE_ID_TTL + Duration::from_secs(1);
        assert!(cache.check_and_record("msg-1", later).is_ok());
    }
}
