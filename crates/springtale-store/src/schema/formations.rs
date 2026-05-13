//! Formation schema — swarms of cooperating agents.
//!
//! Per COOPERATION.pdf: formations are peer groups that coordinate
//! through cadence, momentum, and awareness. A formation is the
//! user-facing abstraction over rules — users think in swarms,
//! the system executes rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

/// A formation stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FormationRow {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A member of a formation (maps to a connector).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FormationMemberRow {
    pub id: String,
    pub formation_id: String,
    pub connector_name: String,
    pub role_hint: Option<String>,
}

/// Formation status values.
pub const STATUS_DRAFT: &str = "draft";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_PAUSED: &str = "paused";
pub const STATUS_DISSOLVED: &str = "dissolved";

/// Momentum state for a formation (replaces config-store hack).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FormationMomentumRow {
    pub formation_id: String,
    pub tier: String,
    pub consecutive_successes: i64,
    pub interference_count: i64,
    pub updated_at: DateTime<Utc>,
}

/// Rally token state for a formation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FormationRallyRow {
    pub formation_id: String,
    pub tokens_remaining: i64,
    pub max_tokens: i64,
}
