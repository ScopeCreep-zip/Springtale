use std::str::FromStr;

use chrono::{DateTime, Utc};
use cron::Schedule;
use tokio::sync::mpsc;

use crate::error::SchedulerError;
use springtale_core::rule::engine::TriggerEvent;

/// A scheduled cron job.
struct CronJob {
    /// Unique name for this job (typically the rule name).
    name: String,
    /// Parsed cron schedule.
    schedule: Schedule,
    /// Handle to the running task (for cancellation).
    handle: tokio::task::JoinHandle<()>,
}

/// Manages cron-scheduled triggers.
///
/// Parses cron expressions, calculates next fire times, and emits
/// `TriggerEvent` to the rule engine's dispatch channel when they fire.
pub struct CronExecutor {
    /// Channel for emitting trigger events to the rule engine.
    trigger_tx: mpsc::Sender<TriggerEvent>,
    /// Active cron jobs.
    jobs: Vec<CronJob>,
}

impl CronExecutor {
    /// Create a new cron executor that sends events to the given channel.
    pub fn new(trigger_tx: mpsc::Sender<TriggerEvent>) -> Self {
        Self {
            trigger_tx,
            jobs: Vec::new(),
        }
    }

    /// Schedule a new cron job.
    ///
    /// The `expression` is a standard cron expression (6 or 7 fields).
    /// When the schedule fires, a `TriggerEvent` with type "Cron" is
    /// sent to the rule engine.
    pub fn schedule(&mut self, name: &str, expression: &str) -> Result<(), SchedulerError> {
        let schedule = Schedule::from_str(expression)
            .map_err(|e| SchedulerError::InvalidCron(format!("{expression}: {e}")))?;

        let tx = self.trigger_tx.clone();
        let job_name = name.to_owned();
        let sched = schedule.clone();

        let handle = tokio::spawn(async move {
            cron_loop(&job_name, &sched, &tx).await;
        });

        self.jobs.push(CronJob {
            name: name.to_owned(),
            schedule,
            handle,
        });

        tracing::info!(name = name, expression = expression, "cron job scheduled");
        Ok(())
    }

    /// Cancel a cron job by name.
    pub fn cancel(&mut self, name: &str) -> bool {
        if let Some(idx) = self.jobs.iter().position(|j| j.name == name) {
            let job = self.jobs.remove(idx);
            job.handle.abort();
            tracing::info!(name = name, "cron job cancelled");
            true
        } else {
            false
        }
    }

    /// Cancel all cron jobs.
    pub fn cancel_all(&mut self) {
        for job in self.jobs.drain(..) {
            job.handle.abort();
        }
    }

    /// List active job names.
    pub fn list(&self) -> Vec<&str> {
        self.jobs.iter().map(|j| j.name.as_str()).collect()
    }

    /// Get the next fire time for an active job by name.
    ///
    /// Uses the stored `Schedule` to calculate when the job will fire next.
    pub fn next_fire_for(&self, name: &str) -> Option<DateTime<Utc>> {
        self.jobs
            .iter()
            .find(|j| j.name == name)
            .and_then(|j| j.schedule.upcoming(Utc).next())
    }

    /// Get the next fire time for a cron expression (for validation/display).
    pub fn next_fire_time(expression: &str) -> Result<Option<DateTime<Utc>>, SchedulerError> {
        let schedule = Schedule::from_str(expression)
            .map_err(|e| SchedulerError::InvalidCron(format!("{expression}: {e}")))?;
        Ok(schedule.upcoming(Utc).next())
    }
}

impl Drop for CronExecutor {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

/// The inner loop for a single cron job.
async fn cron_loop(name: &str, schedule: &Schedule, tx: &mpsc::Sender<TriggerEvent>) {
    loop {
        // Calculate time until next fire
        let now = Utc::now();
        let next = match schedule.upcoming(Utc).next() {
            Some(t) => t,
            None => {
                tracing::warn!(name = name, "cron schedule has no upcoming fire times");
                return;
            }
        };

        let duration = (next - now)
            .to_std()
            .unwrap_or(std::time::Duration::from_secs(1));
        tokio::time::sleep(duration).await;

        // Fire the trigger
        let event = TriggerEvent {
            trigger_type: "Cron".to_owned(),
            connector: None,
            event: Some(name.to_owned()),
            payload: serde_json::json!({
                "fired_at": Utc::now().to_rfc3339(),
                "schedule_name": name,
            }),
        };

        if tx.send(event).await.is_err() {
            // Receiver dropped — scheduler is shutting down
            tracing::debug!(name = name, "cron trigger channel closed, stopping");
            return;
        }

        tracing::debug!(name = name, "cron trigger fired");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_fire_time_valid() {
        let result = CronExecutor::next_fire_time("0 0 * * * *"); // every hour
        assert!(result.is_ok());
        assert!(result.ok().flatten().is_some());
    }

    #[test]
    fn test_next_fire_time_invalid() {
        let result = CronExecutor::next_fire_time("not a cron expression");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_schedule_and_list() {
        let (tx, _rx) = mpsc::channel(10);
        let mut executor = CronExecutor::new(tx);

        let result = executor.schedule("test-job", "* * * * * *"); // every second
        assert!(result.is_ok());
        assert_eq!(executor.list(), vec!["test-job"]);
    }

    #[tokio::test]
    async fn test_cancel() {
        let (tx, _rx) = mpsc::channel(10);
        let mut executor = CronExecutor::new(tx);

        executor.schedule("test-job", "* * * * * *").ok();
        assert!(executor.cancel("test-job"));
        assert!(executor.list().is_empty());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let (tx, _rx) = mpsc::channel(10);
        let mut executor = CronExecutor::new(tx);

        assert!(!executor.cancel("nonexistent"));
    }

    #[tokio::test]
    async fn test_cron_fires_event() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut executor = CronExecutor::new(tx);

        // Schedule every second
        executor.schedule("fast-job", "* * * * * *").ok();

        // Wait for at least one event (with timeout)
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await;

        assert!(result.is_ok(), "timed out waiting for cron event");
        let event = result.ok().flatten();
        assert!(event.is_some());
        let event = event.as_ref();
        assert_eq!(event.map(|e| e.trigger_type.as_str()), Some("Cron"));

        executor.cancel_all();
    }
}
