//! Heartbeat interval — persist and apply in one operation.
//!
//! The interval is durable config, not just a live monitor field: boot
//! reads `heartbeat_interval` back so a restart keeps the setting.

use serde::{Deserialize, Serialize};
use specta::Type;
use springtale_scheduler::HeartbeatMonitor;
use springtale_store::StorageBackend;
use tokio::sync::Mutex;

use crate::error::OperationError;
use crate::operations::config;
use crate::state::RuntimeState;

/// Store config key holding the heartbeat interval in seconds.
pub const HEARTBEAT_CONFIG_KEY: &str = "heartbeat_interval";

/// Live heartbeat state.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct HeartbeatStatus {
    /// Interval in seconds. `0` means disabled.
    pub interval_secs: u64,
    /// Whether the monitor is currently ticking.
    pub enabled: bool,
}

/// Request body for setting the heartbeat interval.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct SetHeartbeatRequest {
    /// Interval in seconds. `0` stops the monitor.
    pub interval_secs: u64,
}

/// Read the live heartbeat state.
pub async fn get(monitor: &Mutex<HeartbeatMonitor>) -> HeartbeatStatus {
    let monitor = monitor.lock().await;
    HeartbeatStatus {
        interval_secs: monitor.interval_secs(),
        enabled: monitor.is_running(),
    }
}

/// Persist the interval, then apply it to the live monitor.
///
/// Persist first: if the store write fails the monitor keeps its old
/// cadence and the caller sees the error, rather than a setting that
/// silently reverts on the next boot.
pub async fn set(
    state: &RuntimeState,
    monitor: &Mutex<HeartbeatMonitor>,
    secs: u64,
) -> Result<HeartbeatStatus, OperationError> {
    config::set_config(&*state.store, HEARTBEAT_CONFIG_KEY, serde_json::json!(secs)).await?;

    let mut monitor = monitor.lock().await;
    if secs == 0 {
        monitor.stop();
    } else {
        monitor.set_interval(secs);
    }
    Ok(HeartbeatStatus {
        interval_secs: monitor.interval_secs(),
        enabled: monitor.is_running(),
    })
}

/// Boot-time read of the persisted interval.
///
/// Falls back to the config-file value when the key was never written
/// or holds something that is not a number.
pub async fn boot_interval(store: &dyn StorageBackend, fallback: u64) -> u64 {
    match config::get_config(store, HEARTBEAT_CONFIG_KEY).await {
        Ok(value) => value.as_u64().unwrap_or(fallback),
        Err(e) => {
            tracing::warn!(error = %e, "heartbeat interval unreadable; using configured default");
            fallback
        }
    }
}
