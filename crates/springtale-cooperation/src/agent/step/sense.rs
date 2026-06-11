//! L0 sense step — scan visible surfaces via `SurfaceSensor` and react to
//! primed ones (`COOPERATION.md §10` stigmergy reflex layer).
//!
//! First step in `AgentLoop::tick`. If a primed surface is in scope, the
//! agent short-circuits to react before reaching the L3/L1 layers.

use crate::agent::context::AgentContext;
use crate::agent::result::AgentTickResult;
use crate::authority;
use crate::awareness::LocalAwareness;
use crate::cadence::ActionDescriptor;
use crate::layer::LayerId;
use crate::stigmergy::trait_::SurfaceSensor;
use crate::stigmergy::types::SurfaceType;

/// Returns `Some(AgentTickResult)` if a primed surface fires; `None` to
/// fall through. Trait-bounded per plan §A2 — `&dyn SurfaceSensor` so the
/// step is mockable independently of the concrete `SurfaceStore`.
pub fn run(
    sensor: &dyn SurfaceSensor,
    awareness: &LocalAwareness,
    ctx: &AgentContext<'_>,
) -> Option<AgentTickResult> {
    if !authority::allows(ctx.momentum.tier, LayerId::L0Ambient) {
        return None;
    }
    let surfaces = sensor.visible_surfaces(awareness);
    let primed = surfaces
        .iter()
        .find(|s| matches!(s.surface_type, SurfaceType::Primed { .. }))?;
    let SurfaceType::Primed { trigger } = &primed.surface_type else {
        return None;
    };
    Some(AgentTickResult {
        agent_id: ctx.agent_id,
        action: Some(ActionDescriptor {
            kind: "surface_reaction".to_owned(),
            target: Some(trigger.kind.clone()),
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
    use super::*;
    use crate::cadence::{AgentId, Tick};
    use crate::context::FormationContext;
    use crate::momentum::{MomentumState, MomentumTier};
    use crate::stigmergy::deposit::SurfaceStore;
    use crate::stigmergy::trait_::SurfaceDeposit;
    use std::time::{Duration, Instant};

    fn ctx<'a>(
        tick: &'a Tick,
        fc: &'a FormationContext,
        m: &'a MomentumState,
        a: &'a crate::attention::AttentionEconomy,
        aw: &'a LocalAwareness,
    ) -> AgentContext<'a> {
        AgentContext {
            agent_id: AgentId::new(),
            tick,
            formation: fc,
            momentum: m,
            attention: a,
            capabilities: &[],
            awareness: aw,
        }
    }

    #[test]
    fn primed_surface_triggers_reaction() {
        let store = SurfaceStore::new();
        store.deposit(
            AgentId::new(),
            SurfaceType::Primed {
                trigger: ActionDescriptor {
                    kind: "rate_limit".into(),
                    target: None,
                    payload_hash: 0,
                },
            },
            serde_json::json!({}),
            None,
            None,
        );
        let aw = LocalAwareness::default();
        let tick = Tick {
            sequence: crate::tick::TickId(0),
            timestamp: Instant::now(),
            window: Duration::from_millis(33),
        };
        let fc = FormationContext::default();
        let m = MomentumState {
            tier: MomentumTier::Hot,
            ..Default::default()
        };
        let attn = crate::attention::AttentionEconomy::new(&[]);
        assert!(run(&store, &aw, &ctx(&tick, &fc, &m, &attn, &aw)).is_some());
    }
}
