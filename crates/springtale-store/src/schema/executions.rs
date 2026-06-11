//! Row types for the `executions` and `execution_steps` tables.
//!
//! These cross the trait + IPC boundaries, so they derive
//! `Serialize + Deserialize` (per the rust-conventions rule:
//! data types that cross boundaries get both). The privacy
//! posture is enforced at write time — these structs have no
//! content fields, only sizes / kinds / refs.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// One row in `executions`. Carries the chain lifecycle for a
/// single rule fire — created when [`ExecutionRecorder::begin`]
/// fires, finalized when the chain completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionRow {
    /// ULID — lex-sortable by start time.
    pub id: String,
    /// Owning agent (None for global rules).
    pub bot_id: Option<String>,
    /// Owning formation (None for solo agents).
    pub formation_id: Option<String>,
    /// Rule this fire belongs to.
    pub rule_id: Option<String>,
    /// Recipe the rule came from (None for ad-hoc / user-built).
    pub recipe_id: Option<String>,
    /// unix ms — when the dispatcher began.
    pub started_at: i64,
    /// unix ms — when the chain finalized; None while running.
    pub finished_at: Option<i64>,
    /// Trigger mode that fired the chain.
    pub mode: ExecutionMode,
    /// Current chain status.
    pub status: ExecutionStatus,
    /// Momentum tier at fire time.
    pub momentum: Option<MomentumTag>,
    /// Short, human-readable trigger description (e.g. "Cron 0 7 * * *").
    pub trigger_summary: Option<String>,
    /// Enum-typed error tag — NEVER the full message (privacy invariant).
    pub error_kind: Option<String>,
    /// Denormalized `finished_at - started_at` in ms.
    pub duration_ms: Option<i64>,
    /// unix ms — when the vacuum task may purge this row.
    pub retention_until: i64,
    /// When non-null, references the previous executions.id this
    /// fire is retrying (Phase C).
    pub retry_of: Option<String>,
}

/// One row in `execution_steps`. Recorded after each `StepOutput`
/// is built by the dispatcher. Sizes-only by default; the two
/// `*_blob_ref` fields are opt-in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionStepRow {
    pub execution_id: String,
    pub step_index: i64,
    pub step_kind: String,
    pub connector: Option<String>,
    pub action: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: StepStatus,
    pub input_bytes: i64,
    pub output_bytes: i64,
    pub output_kind: Option<String>,
    pub error_kind: Option<String>,
    pub input_blob_ref: Option<String>,
    pub output_blob_ref: Option<String>,
}

/// Trigger source that fired the chain.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Cron,
    Webhook,
    ConnectorEvent,
    FileWatch,
    Manual,
    Cooperation,
    Retry,
    DryRun,
}

impl ExecutionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cron => "cron",
            Self::Webhook => "webhook",
            Self::ConnectorEvent => "connector_event",
            Self::FileWatch => "file_watch",
            Self::Manual => "manual",
            Self::Cooperation => "cooperation",
            Self::Retry => "retry",
            Self::DryRun => "dry_run",
        }
    }
}

impl FromStr for ExecutionMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cron" => Ok(Self::Cron),
            "webhook" => Ok(Self::Webhook),
            "connector_event" => Ok(Self::ConnectorEvent),
            "file_watch" => Ok(Self::FileWatch),
            "manual" => Ok(Self::Manual),
            "cooperation" => Ok(Self::Cooperation),
            "retry" => Ok(Self::Retry),
            "dry_run" => Ok(Self::DryRun),
            _ => Err(()),
        }
    }
}

/// Chain status at the row's current moment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Succeeded,
    /// Chain ran to completion but produced no downstream effect
    /// (e.g. dedupe short-circuited). Distinct from `Failed` so
    /// the executions panel surfaces "nothing happened on purpose"
    /// without alarming the user.
    Empty,
    Failed,
    Aborted,
    TimedOut,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Empty => "empty",
            Self::Failed => "failed",
            Self::Aborted => "aborted",
            Self::TimedOut => "timed_out",
        }
    }
}

impl FromStr for ExecutionStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "empty" => Ok(Self::Empty),
            "failed" => Ok(Self::Failed),
            "aborted" => Ok(Self::Aborted),
            "timed_out" => Ok(Self::TimedOut),
            _ => Err(()),
        }
    }
}

/// Per-step status. `Suppressed` is the dedupe / chain-early-exit
/// short-circuit; `Skipped` is reserved for conditional branches
/// (Phase C).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Succeeded,
    Failed,
    Suppressed,
    Skipped,
}

impl StepStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Suppressed => "suppressed",
            Self::Skipped => "skipped",
        }
    }
}

impl FromStr for StepStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "suppressed" => Ok(Self::Suppressed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(()),
        }
    }
}

/// Momentum tag stored alongside an executions row. Mirrors
/// `springtale_cooperation::MomentumTier` but lives in store so
/// the schema doesn't depend on the cooperation crate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MomentumTag {
    Cold,
    Warming,
    Hot,
    Fever,
}

impl MomentumTag {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Warming => "warming",
            Self::Hot => "hot",
            Self::Fever => "fever",
        }
    }
}

impl FromStr for MomentumTag {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cold" => Ok(Self::Cold),
            "warming" => Ok(Self::Warming),
            "hot" => Ok(Self::Hot),
            "fever" => Ok(Self::Fever),
            _ => Err(()),
        }
    }
}

/// Filter passed to [`crate::backend::trait_::StorageBackend::list_executions`].
/// Each field is additive — `None` means "any". Pagination is
/// `limit`/`before` (cursor on `started_at` desc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionFilter {
    pub bot_id: Option<String>,
    pub formation_id: Option<String>,
    pub rule_id: Option<String>,
    pub status: Option<ExecutionStatus>,
    /// Cursor — return rows with `started_at < before`. unix ms.
    pub before: Option<i64>,
    /// Default 50; capped at 500 in the backend.
    pub limit: Option<u32>,
}

/// Compact summary row returned by `list_executions` — strict subset
/// of `ExecutionRow` to keep the wire payload small.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionSummary {
    pub id: String,
    pub bot_id: Option<String>,
    pub formation_id: Option<String>,
    pub rule_id: Option<String>,
    pub recipe_id: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub mode: ExecutionMode,
    pub status: ExecutionStatus,
    pub momentum: Option<MomentumTag>,
    pub trigger_summary: Option<String>,
    pub duration_ms: Option<i64>,
    pub error_kind: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn execution_mode_round_trips_through_str() {
        for m in [
            ExecutionMode::Cron,
            ExecutionMode::Webhook,
            ExecutionMode::ConnectorEvent,
            ExecutionMode::FileWatch,
            ExecutionMode::Manual,
            ExecutionMode::Cooperation,
            ExecutionMode::Retry,
            ExecutionMode::DryRun,
        ] {
            assert_eq!(ExecutionMode::from_str(m.as_str()), Ok(m));
        }
    }

    #[test]
    fn execution_status_round_trips_through_str() {
        for s in [
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
            ExecutionStatus::Empty,
            ExecutionStatus::Failed,
            ExecutionStatus::Aborted,
            ExecutionStatus::TimedOut,
        ] {
            assert_eq!(ExecutionStatus::from_str(s.as_str()), Ok(s));
        }
    }

    #[test]
    fn step_status_round_trips_through_str() {
        for s in [
            StepStatus::Succeeded,
            StepStatus::Failed,
            StepStatus::Suppressed,
            StepStatus::Skipped,
        ] {
            assert_eq!(StepStatus::from_str(s.as_str()), Ok(s));
        }
    }

    #[test]
    fn momentum_tag_round_trips_through_str() {
        for m in [
            MomentumTag::Cold,
            MomentumTag::Warming,
            MomentumTag::Hot,
            MomentumTag::Fever,
        ] {
            assert_eq!(MomentumTag::from_str(m.as_str()), Ok(m));
        }
    }

    #[test]
    fn from_str_returns_err_for_unknown() {
        assert!(ExecutionMode::from_str("nope").is_err());
        assert!(ExecutionStatus::from_str("nope").is_err());
        assert!(StepStatus::from_str("nope").is_err());
        assert!(MomentumTag::from_str("nope").is_err());
    }
}
