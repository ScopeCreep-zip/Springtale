use crate::error::StoreError;
use crate::schema::jobs::{JobId, JobRow};

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn enqueue_job_impl(&self, job: &JobRow) -> Result<JobId, StoreError> {
        let mut jobs = self.jobs.write().await;
        jobs.push(job.clone());
        Ok(job.id)
    }

    pub(super) async fn dequeue_job_impl(&self) -> Result<Option<JobRow>, StoreError> {
        let mut jobs = self.jobs.write().await;
        if jobs.is_empty() {
            return Ok(None);
        }
        // Simple FIFO — remove first pending job
        let idx = jobs.iter().position(|j| j.status == "pending");
        if let Some(i) = idx {
            let mut job = jobs.remove(i);
            job.status = "running".to_owned();
            jobs.push(job.clone());
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    pub(super) async fn complete_job_impl(&self, id: &JobId) -> Result<(), StoreError> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == *id) {
            job.status = "completed".to_owned();
        }
        Ok(())
    }

    pub(super) async fn fail_job_impl(&self, id: &JobId, error: &str) -> Result<(), StoreError> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == *id) {
            job.status = "failed".to_owned();
            job.last_error = Some(error.to_owned());
        }
        Ok(())
    }
}
