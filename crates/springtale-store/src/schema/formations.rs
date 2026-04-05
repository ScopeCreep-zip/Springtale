//! Formation schema — swarms of cooperating agents.
//!
//! Per COOPERATION.pdf: formations are peer groups that coordinate
//! through cadence, momentum, and awareness. A formation is the
//! user-facing abstraction over rules — users think in swarms,
//! the system executes rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A formation stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationRow {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A member of a formation (maps to a connector).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
