//! Timestamp freshness for Kick webhooks (plan 5.2, finding 116).
//!
//! Kick documents `Kick-Event-Message-Id` as an idempotent key and
//! `Kick-Event-Message-Timestamp` as an RFC 3339 send time. This module
//! owns the timestamp half — the cheap, stateless check that a captured
//! request is at least still inside Kick's own signing window. It runs
//! AFTER signature verification, so an unsigned request never reaches it.
//!
//! The message-id half is NOT here any more, and is no longer held in
//! process memory. `KickConnector` exposes the id through
//! `Connector::webhook_replay_key` and the daemon's webhook ingress
//! records it in the store (`springtale_connector::webhook::replay`), so
//! the seen-id set survives a daemon reload, a vault re-lock/unlock and a
//! crash. It used to be a `HashMap` on the connector struct: every
//! restart forgot it and reopened the replay window for every delivery
//! still inside the five-minute skew allowance below.

use crate::error::KickError;

/// Maximum absolute skew between the event timestamp and now.
pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 5 * 60;

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
}
