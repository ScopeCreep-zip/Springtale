use anyhow::{Context, Result};
use tokio::sync::mpsc;

use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;

/// Start scheduler subsystems (cron executor, filesystem watcher, heartbeat monitor).
///
/// Returns the trigger channel pair and the three scheduler components for later
/// ownership by the API state.
pub(super) async fn init_schedulers(
    runtime: &springtale_runtime::RuntimeState,
    heartbeat_interval_secs: u64,
) -> Result<(
    mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
    mpsc::Receiver<springtale_core::rule::engine::TriggerEvent>,
    CronExecutor,
    FsWatcher,
    springtale_scheduler::HeartbeatMonitor,
)> {
    let (trigger_tx, trigger_rx) = mpsc::channel(256);

    let mut cron_executor = CronExecutor::new(trigger_tx.clone());
    let mut fs_watcher =
        FsWatcher::new(trigger_tx.clone()).context("failed to create filesystem watcher")?;

    // Schedule cron triggers and file watches from rules
    {
        let rules = runtime
            .store
            .list_rules()
            .await
            .context("failed to load rules for scheduler")?;
        for rule in &rules {
            if let springtale_core::rule::Trigger::Cron { expression, .. } = &rule.trigger
                && let Err(e) = cron_executor.schedule(&rule.name, expression)
            {
                tracing::warn!(rule = %rule.name, error = %e, "failed to schedule cron trigger");
            }
            if let springtale_core::rule::Trigger::FileWatch { path, .. } = &rule.trigger
                && let Err(e) = fs_watcher.watch(path)
            {
                tracing::warn!(rule = %rule.name, error = %e, "failed to watch path");
            }
        }
    }

    // Start heartbeat monitor (periodic rule evaluation)
    let mut heartbeat_monitor =
        springtale_scheduler::HeartbeatMonitor::new(heartbeat_interval_secs, trigger_tx.clone());
    if heartbeat_interval_secs > 0 {
        heartbeat_monitor.start();
        tracing::info!(
            interval_secs = heartbeat_interval_secs,
            "heartbeat monitor started"
        );
    }

    tracing::info!(
        cron_jobs = cron_executor.list().len(),
        watched_paths = fs_watcher.watched_paths().len(),
        "scheduler started"
    );

    Ok((
        trigger_tx,
        trigger_rx,
        cron_executor,
        fs_watcher,
        heartbeat_monitor,
    ))
}
