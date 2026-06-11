use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use serde_json::Value;

use crate::awareness::LocalAwareness;
use crate::cadence::AgentId;

use super::awareness_match;
use super::trait_::{SurfaceDeposit, SurfaceSensor};
use super::types::{Surface, SurfaceType};

/// RCU-style surface store: writers clone-and-swap the vector; readers take
/// cheap Arc snapshots. Matches COOPERATION.md §10's `ArcSwap` guidance.
#[derive(Debug, Default)]
pub struct SurfaceStore {
    surfaces: ArcSwap<Vec<Surface>>,
}

impl SurfaceStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Arc<Vec<Surface>> {
        self.surfaces.load_full()
    }

    pub(super) fn replace(&self, new: Vec<Surface>) {
        self.surfaces.store(Arc::new(new));
    }

    pub fn len(&self) -> usize {
        self.surfaces.load().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SurfaceDeposit for SurfaceStore {
    fn deposit(
        &self,
        created_by: AgentId,
        surface_type: SurfaceType,
        data: Value,
        ttl: Option<Duration>,
        capability: Option<crate::capability::CapabilityDecl>,
    ) -> Surface {
        let now = Instant::now();
        let surface = Surface {
            id: uuid::Uuid::new_v4(),
            created_by,
            surface_type,
            data,
            expires: ttl.map(|d| now + d),
            capability,
        };
        let current = self.surfaces.load();
        let mut next: Vec<Surface> = (**current).clone();
        next.push(surface.clone());
        self.surfaces.store(Arc::new(next));
        surface
    }

    fn decay(&self, now: Instant) {
        super::decay::sweep(self, now);
    }
}

impl SurfaceSensor for SurfaceStore {
    fn visible_surfaces(&self, awareness: &LocalAwareness) -> Vec<Surface> {
        awareness_match::visible(awareness, &self.snapshot())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn deposit_returns_surface_with_id() {
        let store = SurfaceStore::new();
        let agent = AgentId::new();
        let s = store.deposit(
            agent,
            SurfaceType::Substrate,
            serde_json::json!({"element": "water"}),
            None,
            None,
        );
        assert_eq!(s.created_by, agent);
        assert!(s.expires.is_none());
    }

    #[test]
    fn deposit_increments_len() {
        let store = SurfaceStore::new();
        assert!(store.is_empty());
        store.deposit(
            AgentId::new(),
            SurfaceType::Substrate,
            serde_json::json!({}),
            None,
            None,
        );
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn deposit_with_ttl_sets_expiry() {
        let store = SurfaceStore::new();
        let s = store.deposit(
            AgentId::new(),
            SurfaceType::Active {
                remaining: Duration::from_secs(10),
            },
            serde_json::json!({}),
            Some(Duration::from_secs(5)),
            None,
        );
        assert!(s.expires.is_some());
    }

    #[test]
    fn decay_removes_expired_surfaces() {
        let store = SurfaceStore::new();
        store.deposit(
            AgentId::new(),
            SurfaceType::Substrate,
            serde_json::json!({}),
            Some(Duration::from_millis(0)),
            None,
        );
        store.deposit(
            AgentId::new(),
            SurfaceType::Substrate,
            serde_json::json!({}),
            None,
            None,
        );
        store.decay(Instant::now() + Duration::from_secs(1));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn capability_tag_preserved() {
        use crate::cadence::ActionDescriptor;
        use crate::capability::CapabilityDecl;
        let store = SurfaceStore::new();
        let s = store.deposit(
            AgentId::new(),
            SurfaceType::Primed {
                trigger: ActionDescriptor {
                    kind: "fire".into(),
                    target: None,
                    payload_hash: 0,
                },
            },
            serde_json::json!({}),
            None,
            Some(CapabilityDecl::new("github")),
        );
        assert_eq!(
            s.capability.as_ref().map(|c| c.name.as_str()),
            Some("github")
        );
    }
}
