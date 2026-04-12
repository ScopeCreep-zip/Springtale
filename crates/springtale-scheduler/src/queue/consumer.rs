use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::sync::mpsc;

use super::producer::{Job, JobStatus};
use crate::error::SchedulerError;

/// A boxed future — same as `futures_core::BoxFuture` but without the dep.
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Callback for processing a job. Returns a boxed future.
pub type JobHandler = Arc<dyn Fn(Job) -> BoxFuture<Result<(), String>> + Send + Sync>;

/// Consumes (dequeues) and processes jobs from the queue.
///
/// Supports configurable concurrency limits via a semaphore.
pub struct JobConsumer {
    rx: mpsc::Receiver<Job>,
    concurrency: Arc<Semaphore>,
    handler: Option<JobHandler>,
}

impl JobConsumer {
    /// Create a new consumer with the given channel and concurrency limit.
    pub fn new(rx: mpsc::Receiver<Job>, max_concurrent: usize) -> Self {
        Self {
            rx,
            concurrency: Arc::new(Semaphore::new(max_concurrent)),
            handler: None,
        }
    }

    /// Set the job handler.
    pub fn set_handler(&mut self, handler: JobHandler) {
        self.handler = Some(handler);
    }

    /// Run the consumer loop. Processes jobs until the channel closes.
    ///
    /// Each job is processed in a separate tokio task, bounded by the
    /// concurrency semaphore.
    pub async fn run(&mut self) -> Result<(), SchedulerError> {
        let handler = self
            .handler
            .take()
            .ok_or_else(|| SchedulerError::QueueError("no job handler set".into()))?;

        while let Some(mut job) = self.rx.recv().await {
            let permit = self
                .concurrency
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| SchedulerError::QueueError("semaphore closed".into()))?;

            let h = Arc::clone(&handler);

            tokio::spawn(async move {
                job.status = JobStatus::Running;
                job.started_at = Some(chrono::Utc::now().to_rfc3339());
                job.attempts += 1;

                tracing::debug!(
                    job_id = %job.id,
                    attempt = job.attempts,
                    "processing job"
                );

                match h(job.clone()).await {
                    Ok(()) => {
                        tracing::debug!(job_id = %job.id, "job completed");
                    }
                    Err(e) => {
                        tracing::warn!(
                            job_id = %job.id,
                            attempt = job.attempts,
                            error = %e,
                            "job failed"
                        );
                    }
                }

                drop(permit); // release concurrency slot
            });
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::producer::{Job, JobProducer};
    use super::*;

    #[tokio::test]
    async fn test_consumer_processes_job() {
        let (tx, rx) = mpsc::channel(10);
        let producer = JobProducer::new(tx.clone());
        let mut consumer = JobConsumer::new(rx, 4);

        let (result_tx, mut result_rx) = mpsc::channel(10);

        consumer.set_handler(Arc::new(move |job: Job| {
            let rtx = result_tx.clone();
            Box::pin(async move {
                rtx.send(job.id).await.ok();
                Ok(())
            })
        }));

        // Enqueue a job
        let job_id = producer
            .enqueue(serde_json::json!({"test": true}), 1)
            .await
            .unwrap();

        // Drop ALL senders to close the channel so consumer.run() exits
        drop(producer);
        drop(tx);

        // Run consumer (will process one job then exit when channel closes)
        let consumer_handle = tokio::spawn(async move { consumer.run().await });

        // Check we got the result
        let received =
            tokio::time::timeout(std::time::Duration::from_secs(2), result_rx.recv()).await;

        assert!(received.is_ok());
        assert_eq!(received.ok().flatten(), Some(job_id));

        consumer_handle.await.ok();
    }
}
