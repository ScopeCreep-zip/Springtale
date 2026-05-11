//! Wire types for the cross-formation gossip bus (G6).
//!
//! `FormationView` is the soft-state snapshot every formation broadcasts;
//! `FormationOutcome` is the terminal value emitted on dissolve;
//! `FormationDelta` is the union peers see on the subscriber stream.
//!
//! All types are `Serialize + Deserialize` so a chitchat-backed implementation
//! can encode them on the KV substrate. Every field is small and chosen for
//! game-state observability, not full state replication — peers only need
//! "what's happening over there" not "exactly which task each agent claimed".

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cadence::IntentPattern;
use crate::momentum::MomentumTier;
use crate::types::FormationId;

/// Broadcast snapshot of one formation's current state. Republished
/// whenever the formation's intent changes, momentum tier flips, or
/// rally tokens cross a threshold.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../tauri/packages/types/src/generated/")]
pub struct FormationView {
    #[ts(type = "string")]
    pub formation_id: FormationId,
    #[ts(type = "string")]
    pub intent: IntentPattern,
    #[ts(type = "string")]
    pub momentum_tier: MomentumTier,
    pub operational_count: u32,
    pub member_count: u32,
    pub rally_tokens_remaining: u32,
    pub status: FormationStatus,
    pub at: chrono::DateTime<chrono::Utc>,
}

/// Lifecycle state communicated via gossip. Mirrors the colony-canvas
/// `status` enum exposed by the runtime API so peer formations and the
/// UI agree on terminology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../tauri/packages/types/src/generated/")]
pub enum FormationStatus {
    Draft,
    Active,
    Paused,
    Dissolved,
}

/// Terminal outcome published once on dissolve. Drives "what just
/// finished" awareness for sibling formations on the same connector
/// graph and feeds the global mental-model persistence layer (G2).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../tauri/packages/types/src/generated/")]
pub struct FormationOutcome {
    #[ts(type = "string")]
    pub formation_id: FormationId,
    #[ts(type = "string")]
    pub final_intent: IntentPattern,
    pub success_count: u32,
    pub failure_count: u32,
    pub dissolve_reason: String,
    pub at: chrono::DateTime<chrono::Utc>,
}

/// Stream item peer formations receive. Covers both the running-state
/// snapshot stream and the terminal-outcome stream so subscribers don't
/// have to wire two channels.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../tauri/packages/types/src/generated/")]
pub enum FormationDelta {
    View(FormationView),
    Outcome(FormationOutcome),
}

impl FormationDelta {
    pub fn formation_id(&self) -> FormationId {
        match self {
            FormationDelta::View(v) => v.formation_id,
            FormationDelta::Outcome(o) => o.formation_id,
        }
    }
}
