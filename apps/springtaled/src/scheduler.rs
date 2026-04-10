//! Trigger scheduling — cron and filesystem watcher management.
//!
//! App-specific: cron and fs_watcher are springtaled concerns, not
//! shared runtime. The desktop app handles scheduling differently
//! (Tauri lifecycle, not background tasks).

use std::sync::Arc;

use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;
use tokio::sync::Mutex;

/// Manages cron and filesystem trigger scheduling.
///
/// Extracted from inline handler logic in `api/rules.rs` so that
/// schedule/unschedule operations are reusable and testable.
#[derive(Clone)]
pub struct AppScheduler {
    pub cron: Arc<Mutex<CronExecutor>>,
    pub fs_watcher: Arc<Mutex<FsWatcher>>,
}

impl AppScheduler {
    /// Schedule a rule's trigger (cron or file watch).
    ///
    /// No-op for trigger types that don't need scheduling (webhook, connector event).
    pub async fn schedule(&self, rule: &springtale_core::rule::types::Rule) -> Result<(), String> {
        match &rule.trigger {
            springtale_core::rule::Trigger::Cron { expression } => {
                let mut cron = self.cron.lock().await;
                cron.schedule(&rule.name, expression)
                    .map_err(|e| format!("failed to schedule cron trigger: {e}"))?;
            }
            springtale_core::rule::Trigger::FileWatch { path, .. } => {
                let mut watcher = self.fs_watcher.lock().await;
                watcher
                    .watch(path)
                    .map_err(|e| format!("failed to watch path: {e}"))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Unschedule a rule's trigger.
    pub async fn unschedule(&self, rule: &springtale_core::rule::types::Rule) {
        match &rule.trigger {
            springtale_core::rule::Trigger::Cron { .. } => {
                let mut cron = self.cron.lock().await;
                if cron.cancel(&rule.name) {
                    tracing::info!(rule = %rule.name, "cancelled cron trigger");
                }
            }
            springtale_core::rule::Trigger::FileWatch { path, .. } => {
                let mut watcher = self.fs_watcher.lock().await;
                if let Err(e) = watcher.unwatch(path) {
                    tracing::warn!(rule = %rule.name, error = %e, "failed to unwatch path");
                }
            }
            _ => {}
        }
    }
}
