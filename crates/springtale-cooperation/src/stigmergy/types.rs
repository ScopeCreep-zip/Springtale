use std::time::{Duration, Instant};

use serde_json::Value;

use crate::cadence::{ActionDescriptor, AgentId};
use crate::capability::CapabilityDecl;

pub type SurfaceId = uuid::Uuid;

/// Surface lifecycle — Divinity's elemental combo system, COOPERATION.md §10.
#[derive(Debug, Clone)]
pub enum SurfaceType {
    /// Passive surface. Divinity: water on the ground.
    Substrate,
    /// Ready to be triggered by another agent's action. Divinity: oil ready to ignite.
    Primed { trigger: ActionDescriptor },
    /// Active effect with remaining duration. Divinity: fire burning.
    Active { remaining: Duration },
}

/// A deposited surface in the formation environment.
///
/// Spec-faithful to COOPERATION.md §10 (Substrate / Primed / Active, data
/// payload, optional expiry). Adds `capability`: when `Some`, only agents
/// whose awareness matches that capability perceive it; `None` is broadcast.
#[derive(Debug, Clone)]
pub struct Surface {
    pub id: SurfaceId,
    pub created_by: AgentId,
    pub surface_type: SurfaceType,
    pub data: Value,
    pub expires: Option<Instant>,
    pub capability: Option<CapabilityDecl>,
}
