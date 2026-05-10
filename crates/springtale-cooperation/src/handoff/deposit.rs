//! Durable environment-mediated handoff — routes `HandoffType::EnvironmentMediated`
//! through the workspace `StorageBackend` so deposits survive process restarts
//! and the TTL sweeper reclaims abandoned drops.
//!
//! Per COOPERATION.md §20.3: the in-memory `Workspace` satisfies the
//! fast-path semantics for same-process handoffs. For cross-restart
//! durability and TTL cleanup, the spec proposes sled; Springtale routes
//! the same semantics through SQLite (`coop_deposits` table) because all
//! cooperation SQL must live in `springtale-store` (CLAUDE.md).

use std::sync::Arc;
use std::time::Duration;

use springtale_store::StorageBackend;

use crate::cadence::AgentId;
use crate::error::{CooperationError, HandoffError};

/// Serialize the handoff payload JSON and deposit it at `location`.
/// `ttl` is optional — `None` means the deposit stays until collected
/// or manually swept.
pub async fn deposit(
    store: &Arc<dyn StorageBackend>,
    location: &str,
    payload: &serde_json::Value,
    depositor: AgentId,
    ttl: Option<Duration>,
) -> Result<(), CooperationError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|e| HandoffError::SerializeDeposit(e.to_string()))?;
    let ttl_secs = ttl.map(|d| d.as_secs() as i64);
    store
        .coop_deposit(location, &bytes, &depositor.0.to_string(), ttl_secs)
        .await?;
    Ok(())
}

/// Atomically collect the payload at `location` for `collector`.
/// Returns `Ok(None)` if the location is empty or already claimed —
/// exactly-once claim is enforced by the backend (`UPDATE ... RETURNING`
/// for SQLite, `DashMap::remove` for in-memory).
pub async fn collect(
    store: &Arc<dyn StorageBackend>,
    location: &str,
    collector: AgentId,
) -> Result<Option<serde_json::Value>, CooperationError> {
    let Some(bytes) = store
        .coop_collect(location, &collector.0.to_string())
        .await?
    else {
        return Ok(None);
    };
    let value = serde_json::from_slice(&bytes)
        .map_err(|e| HandoffError::DeserializeDeposit(e.to_string()))?;
    Ok(Some(value))
}

/// Sweep expired deposits. Called on a timer from runtime init.
pub async fn sweep(store: &Arc<dyn StorageBackend>) -> Result<u64, CooperationError> {
    Ok(store.coop_sweep_expired().await?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::backend::InMemoryBackend;

    #[tokio::test]
    async fn deposit_then_collect_roundtrip() {
        let store: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let depositor = AgentId::new();
        let collector = AgentId::new();
        let value = serde_json::json!({"result": 42});

        deposit(&store, "loc", &value, depositor, None).await.unwrap();
        let got = collect(&store, "loc", collector).await.unwrap();
        assert_eq!(got, Some(value));
    }

    #[tokio::test]
    async fn exactly_once_claim() {
        let store: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let depositor = AgentId::new();
        let c1 = AgentId::new();
        let c2 = AgentId::new();
        deposit(&store, "loc", &serde_json::json!("x"), depositor, None)
            .await
            .unwrap();
        let first = collect(&store, "loc", c1).await.unwrap();
        let second = collect(&store, "loc", c2).await.unwrap();
        assert!(first.is_some());
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn sweep_removes_expired() {
        let store: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let depositor = AgentId::new();
        // TTL of 0s — expires immediately.
        deposit(
            &store,
            "loc",
            &serde_json::json!("x"),
            depositor,
            Some(Duration::from_secs(0)),
        )
        .await
        .unwrap();
        // Wait one second so the deposit is strictly past its expires_at.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let swept = sweep(&store).await.unwrap();
        assert_eq!(swept, 1);
    }
}
