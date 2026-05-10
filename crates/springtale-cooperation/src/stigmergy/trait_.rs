use std::time::{Duration, Instant};

use serde_json::Value;

use crate::awareness::LocalAwareness;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

use super::types::{Surface, SurfaceType};

/// Read-only perception — agents query what they can currently sense.
pub trait SurfaceSensor: Send + Sync {
    fn visible_surfaces(&self, awareness: &LocalAwareness) -> Vec<Surface>;
}

/// Write-side interface — the environment accepts deposits and decays them.
pub trait SurfaceDeposit: Send + Sync {
    fn deposit(
        &self,
        created_by: AgentId,
        surface_type: SurfaceType,
        data: Value,
        ttl: Option<Duration>,
        capability: Option<CapabilityDecl>,
    ) -> Surface;
    fn decay(&self, now: Instant);
}

/// Combined facade — a stigmergy substrate that supports both reads and
/// writes. `Formation::surfaces` uses `Arc<dyn SurfaceSubstrate>` so the
/// concrete impl (production `deposit::SurfaceStore`, in-test mocks) is
/// swappable per plan §B4. The blanket impl removes the need for callers
/// to `as &dyn SurfaceSensor` / `as &dyn SurfaceDeposit` repeatedly.
pub trait SurfaceSubstrate: SurfaceDeposit + SurfaceSensor + Send + Sync {}

impl<T: SurfaceDeposit + SurfaceSensor + Send + Sync + ?Sized> SurfaceSubstrate for T {}
