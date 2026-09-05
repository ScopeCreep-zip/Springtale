//! End-to-end SQLite-backed quota persistence (Phase-7 audit D).
//!
//! Validates daemon-restart survival: a `SqliteTokenQuota` carries a
//! per-bot counter forward across a fresh handle pointing at the
//! same SQLite file. The in-memory `InMemoryTokenQuota` cannot —
//! this is what the new persistence buys us.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use springtale_ai::{QuotaCheck, TokenQuota};
use springtale_runtime::SqliteTokenQuota;
use springtale_store::StorageBackend;
use springtale_store::backend::SqliteBackend;
use tempfile::tempdir;

#[tokio::test]
async fn quota_counter_survives_handle_drop() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("quota.db");

    // First "boot": open a backend, build a quota, reserve some
    // tokens, drop everything.
    {
        let backend: Arc<dyn StorageBackend> =
            Arc::new(SqliteBackend::open_encrypted(&path, TEST_KEY_HEX).unwrap());
        let quota = SqliteTokenQuota::new(backend, Some(1_000));
        let r = quota.check_and_reserve("bot-1", 300).await.unwrap();
        assert!(matches!(r, QuotaCheck::Allowed { remaining: 700 }));
        // Drop the quota + backend at scope end.
    }

    // Second "boot": fresh handles over the same SQLite file. The
    // counter must persist so the bot can't get a fresh 1,000-token
    // budget across a restart.
    {
        let backend: Arc<dyn StorageBackend> =
            Arc::new(SqliteBackend::open_encrypted(&path, TEST_KEY_HEX).unwrap());
        let quota = SqliteTokenQuota::new(backend, Some(1_000));
        assert_eq!(quota.usage("bot-1").await.unwrap(), 300);

        // Reserve right up to the cap; must succeed at remaining=0.
        let r = quota.check_and_reserve("bot-1", 700).await.unwrap();
        assert!(matches!(r, QuotaCheck::Allowed { remaining: 0 }));

        // One more byte → Denied. This proves the persisted counter
        // is the gating value, not a fresh-boot reset.
        let r = quota.check_and_reserve("bot-1", 1).await.unwrap();
        assert!(matches!(
            r,
            QuotaCheck::Denied {
                used: 1_000,
                limit: 1_000,
            }
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_commits_do_not_lose_updates() {
    // OWASP LLM10 correctness: two AI calls for the same bot that
    // finish at the same instant must both have their commit deltas
    // applied. The atomic ai_token_usage_commit backend method
    // serialises read-adjust-write under the connection lock.
    //
    // Setup: pre-reserve 1000 tokens, then issue N concurrent
    // commits each replacing a 100-prior with a 30-actual (i.e.
    // delta = -70 per commit). The expected final counter is
    // 1000 - (N * 70).
    use std::sync::Arc;

    let dir = tempdir().unwrap();
    let path = dir.path().join("quota.db");
    let backend: Arc<dyn StorageBackend> =
        Arc::new(SqliteBackend::open_encrypted(&path, TEST_KEY_HEX).unwrap());
    let quota = Arc::new(SqliteTokenQuota::new(backend, None));

    let total_reservation: u64 = 1000;
    quota
        .check_and_reserve("bot-race", total_reservation)
        .await
        .unwrap();
    assert_eq!(quota.usage("bot-race").await.unwrap(), 1000);

    const N: u64 = 10;
    let prior_per_commit: u64 = 100;
    let actual_per_commit: u64 = 30;

    let mut handles = Vec::new();
    for _ in 0..N {
        let q = quota.clone();
        handles.push(tokio::spawn(async move {
            q.commit("bot-race", prior_per_commit, actual_per_commit)
                .await
                .unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let expected = total_reservation - (N * (prior_per_commit - actual_per_commit));
    assert_eq!(
        quota.usage("bot-race").await.unwrap(),
        expected,
        "concurrent commits must apply every delta — no lost updates"
    );
}

#[tokio::test]
async fn quota_unlimited_observes_usage_without_denial() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("quota.db");
    let backend: Arc<dyn StorageBackend> =
        Arc::new(SqliteBackend::open_encrypted(&path, TEST_KEY_HEX).unwrap());
    let quota = SqliteTokenQuota::new(backend, None);

    let r = quota.check_and_reserve("bot-x", 50_000).await.unwrap();
    assert!(matches!(r, QuotaCheck::Allowed { .. }));
    assert_eq!(quota.usage("bot-x").await.unwrap(), 50_000);

    // commit lowers the counter to the actual usage.
    quota.commit("bot-x", 50_000, 8_000).await.unwrap();
    assert_eq!(quota.usage("bot-x").await.unwrap(), 8_000);
}

/// Production stores are always encrypted (plan 0.5), so file-backed
/// tests open with a fixed key. Never used outside tests.
const TEST_KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
