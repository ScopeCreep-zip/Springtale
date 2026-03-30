use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::StoreError;
use crate::schema::connectors::ConnectorRow;
use crate::schema::events::{EventEntry, EventFilter};
use crate::schema::jobs::{JobId, JobRow};
use springtale_core::rule::types::{Rule, RuleId};

/// Persistence backend for all Springtale data.
///
/// Phase 1a: SQLite (single file, zero external dependencies).
/// All crates access persistence through this trait. No raw SQL
/// outside the store crate.
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    // ── Rules ──────────────────────────────────────────────────

    /// Insert a new rule. Returns the assigned RuleId.
    async fn insert_rule(&self, rule: &Rule) -> Result<RuleId, StoreError>;

    /// Find all rules that match a given trigger type.
    async fn find_rules_by_trigger(&self, trigger_type: &str) -> Result<Vec<Rule>, StoreError>;

    /// List all rules.
    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError>;

    /// Toggle a rule's status (enabled/disabled).
    async fn toggle_rule(&self, id: &RuleId, enabled: bool) -> Result<(), StoreError>;

    /// Delete a rule.
    async fn delete_rule(&self, id: &RuleId) -> Result<(), StoreError>;

    // ── Connectors ─────────────────────────────────────────────

    /// Register a connector's manifest.
    async fn register_connector(&self, row: &ConnectorRow) -> Result<(), StoreError>;

    /// List all registered connectors.
    async fn list_connectors(&self) -> Result<Vec<ConnectorRow>, StoreError>;

    /// Enable or disable a connector.
    async fn set_connector_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError>;

    /// Remove a connector.
    async fn remove_connector(&self, name: &str) -> Result<(), StoreError>;

    // ── Events ─────────────────────────────────────────────────

    /// Log an event (audit trail entry).
    async fn log_event(&self, event: &EventEntry) -> Result<(), StoreError>;

    /// List events matching a filter.
    async fn list_events(&self, filter: &EventFilter) -> Result<Vec<EventEntry>, StoreError>;

    /// Delete events older than a given timestamp (retention enforcement).
    async fn delete_events_before(&self, before: &DateTime<Utc>) -> Result<u64, StoreError>;

    // ── Jobs ───────────────────────────────────────────────────

    /// Enqueue a new job.
    async fn enqueue_job(&self, job: &JobRow) -> Result<JobId, StoreError>;

    /// Dequeue the next pending job (marks it as running).
    async fn dequeue_job(&self) -> Result<Option<JobRow>, StoreError>;

    /// Mark a job as complete.
    async fn complete_job(&self, id: &JobId) -> Result<(), StoreError>;

    /// Mark a job as failed with an error message.
    async fn fail_job(&self, id: &JobId, error: &str) -> Result<(), StoreError>;
}
