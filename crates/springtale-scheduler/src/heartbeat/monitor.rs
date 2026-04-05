use std::time::Duration;

use tokio::sync::{mpsc, watch};

use springtale_core::rule::engine::TriggerEvent;

/// Proactive wake cycle — evaluates rules on a fixed interval.
///
/// From ARCHITECTURE.md §6.5:
/// > "Every interval (default 30 minutes), the heartbeat executor runs
/// > a configured set of rules through springtale-core's pipeline engine."
///
/// Unlike cron triggers (which fire at specific times), the heartbeat
/// fires at a fixed interval from when it was started. This is simpler
/// and more predictable for users who just want "check every N minutes."
///
/// For IPV survivors: configure heartbeat to periodically check if a
/// safety contact has responded, without requiring manual intervention.
pub struct HeartbeatMonitor {
    interval: Duration,
    trigger_tx: mpsc::Sender<TriggerEvent>,
    shutdown_tx: Option<watch::Sender<bool>>,
}

impl HeartbeatMonitor {
    /// Create a new heartbeat monitor.
    ///
    /// Does NOT start automatically — call `start()` to begin the cycle.
    pub fn new(interval_secs: u64, trigger_tx: mpsc::Sender<TriggerEvent>) -> Self {
        Self {
            interval: Duration::from_secs(interval_secs),
            trigger_tx,
            shutdown_tx: None,
        }
    }

    /// Start the heartbeat cycle.
    ///
    /// Spawns a background task that sends a `TriggerEvent` with type
    /// "Heartbeat" to the rule engine on each interval tick.
    pub fn start(&mut self) {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let interval = self.interval;
        let trigger_tx = self.trigger_tx.clone();

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            // Skip the first immediate tick — first heartbeat fires after one full interval
            timer.tick().await;

            loop {
                tokio::select! {
                    _ = timer.tick() => {
                        let event = TriggerEvent {
                            trigger_type: "Heartbeat".to_owned(),
                            connector: None,
                            event: None,
                            payload: serde_json::json!({
                                "source": "heartbeat",
                                "interval_secs": interval.as_secs(),
                            }),
                        };

                        if let Err(e) = trigger_tx.send(event).await {
                            tracing::warn!(error = %e, "heartbeat trigger send failed");
                            break;
                        }

                        tracing::debug!(
                            interval_secs = interval.as_secs(),
                            "heartbeat fired"
                        );
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("heartbeat monitor shutting down");
                        break;
                    }
                }
            }
        });

        self.shutdown_tx = Some(shutdown_tx);
        tracing::info!(
            interval_secs = self.interval.as_secs(),
            "heartbeat monitor started"
        );
    }

    /// Reconfigure the heartbeat interval.
    ///
    /// Stops the current cycle and starts a new one with the updated interval.
    pub fn set_interval(&mut self, interval_secs: u64) {
        self.stop();
        self.interval = Duration::from_secs(interval_secs);
        self.start();
    }

    /// Stop the heartbeat cycle.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
    }

    /// Get the current interval in seconds.
    pub fn interval_secs(&self) -> u64 {
        self.interval.as_secs()
    }

    /// Check if the heartbeat is currently running.
    pub fn is_running(&self) -> bool {
        self.shutdown_tx.is_some()
    }
}

impl Drop for HeartbeatMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heartbeat_fires() {
        let (trigger_tx, mut trigger_rx) = mpsc::channel(10);
        let mut monitor = HeartbeatMonitor::new(1, trigger_tx);

        monitor.start();

        // Wait for first heartbeat (after 1 second interval)
        let event = tokio::time::timeout(Duration::from_secs(3), trigger_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(event.trigger_type, "Heartbeat");
        monitor.stop();
    }

    #[tokio::test]
    async fn test_heartbeat_stop() {
        let (trigger_tx, _trigger_rx) = mpsc::channel(10);
        let mut monitor = HeartbeatMonitor::new(1, trigger_tx);

        monitor.start();
        assert!(monitor.is_running());

        monitor.stop();
        assert!(!monitor.is_running());
    }

    #[tokio::test]
    async fn test_heartbeat_set_interval() {
        let (trigger_tx, _trigger_rx) = mpsc::channel(10);
        let mut monitor = HeartbeatMonitor::new(60, trigger_tx);

        assert_eq!(monitor.interval_secs(), 60);

        monitor.start();
        monitor.set_interval(30);

        assert_eq!(monitor.interval_secs(), 30);
        assert!(monitor.is_running());

        monitor.stop();
    }
}
