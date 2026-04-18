//! L0 sense step — scan visible surfaces and react to primed ones.
//!
//! First step in the agent tick pipeline. If a primed surface matches
//! the agent's awareness, the agent short-circuits to react to it before
//! scanning the blackboard. This implements the stigmergy "reflex" layer.

use crate::agent_loop::AgentTickResult;
use crate::authority;
use crate::awareness::LocalAwareness;
use crate::cadence::{ActionDescriptor, AgentId};
use crate::layer::LayerId;
use crate::momentum::MomentumTier;
use crate::stigmergy::deposit::SurfaceStore;
use crate::stigmergy::trait_::SurfaceSensor;
use crate::stigmergy::types::SurfaceType;

/// Check for primed surfaces this agent can perceive. Returns a tick result
/// if a surface trigger fires; `None` to fall through to the next step.
pub fn step_sense(
    store: &SurfaceStore,
    awareness: &LocalAwareness,
    agent_id: AgentId,
    tier: MomentumTier,
) -> Option<AgentTickResult> {
    if !authority::allows(tier, LayerId::L0Ambient) {
        return None;
    }

    let surfaces = store.visible_surfaces(awareness);
    let primed = surfaces
        .iter()
        .find(|s| matches!(s.surface_type, SurfaceType::Primed { .. }));

    let trigger = match primed {
        Some(s) => match &s.surface_type {
            SurfaceType::Primed { trigger } => trigger.clone(),
            _ => return None,
        },
        None => return None,
    };

    Some(AgentTickResult {
        agent_id,
        action: Some(ActionDescriptor {
            kind: "surface_reaction".to_owned(),
            target: Some(trigger.kind),
            payload_hash: 0,
        }),
        alignment: 1.0,
        interference_with: vec![],
        task_claimed: None,
        task_completed: false,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::stigmergy::trait_::SurfaceDeposit;

    #[test]
    fn no_surfaces_returns_none() {
        let store = SurfaceStore::new();
        let awareness = LocalAwareness::default();
        assert!(step_sense(&store, &awareness, AgentId::new(), MomentumTier::Hot).is_none());
    }

    #[test]
    fn primed_broadcast_surface_triggers_reaction() {
        let store = SurfaceStore::new();
        store.deposit(
            AgentId::new(),
            SurfaceType::Primed {
                trigger: ActionDescriptor {
                    kind: "api_rate_limit".into(),
                    target: None,
                    payload_hash: 0,
                },
            },
            serde_json::json!({}),
            None,
            None, // broadcast — visible to all
        );
        let awareness = LocalAwareness::default();
        let result = step_sense(&store, &awareness, AgentId::new(), MomentumTier::Hot);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.action.as_ref().unwrap().kind, "surface_reaction");
        assert_eq!(
            r.action.as_ref().unwrap().target.as_deref(),
            Some("api_rate_limit")
        );
    }

    #[test]
    fn substrate_surface_does_not_trigger() {
        let store = SurfaceStore::new();
        store.deposit(
            AgentId::new(),
            SurfaceType::Substrate,
            serde_json::json!({}),
            None,
            None,
        );
        let awareness = LocalAwareness::default();
        assert!(step_sense(&store, &awareness, AgentId::new(), MomentumTier::Hot).is_none());
    }

    #[test]
    fn capability_tagged_surface_hidden_from_unmatched_agent() {
        let store = SurfaceStore::new();
        store.deposit(
            AgentId::new(),
            SurfaceType::Primed {
                trigger: ActionDescriptor {
                    kind: "test".into(),
                    target: None,
                    payload_hash: 0,
                },
            },
            serde_json::json!({}),
            Some(Duration::from_secs(60)),
            Some(crate::capability::CapabilityDecl::new("github")),
        );
        let awareness = LocalAwareness::default();
        assert!(step_sense(&store, &awareness, AgentId::new(), MomentumTier::Hot).is_none());
    }
}
