use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Row type for the `events` table (audit trail).
///
/// Stores trigger type, connector name, timestamp, and action taken.
/// Does NOT store trigger payload content — privacy requirement.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EventEntry {
    /// Unique event identifier.
    pub id: Uuid,
    /// Name of the connector that produced this event.
    pub connector_name: String,
    /// Trigger type (e.g., "Cron", "FileWatch", "ConnectorEvent").
    pub trigger_type: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Human-readable description of the action taken.
    pub action_taken: String,
}

/// Filter parameters for querying events.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Filter by connector name.
    pub connector_name: Option<String>,
    /// Filter by trigger type.
    pub trigger_type: Option<String>,
    /// Return events after this time.
    pub after: Option<DateTime<Utc>>,
    /// Return events before this time.
    pub before: Option<DateTime<Utc>>,
    /// Maximum number of events to return.
    pub limit: Option<u32>,
    /// Number of events to skip (for pagination).
    pub offset: Option<u32>,
}
