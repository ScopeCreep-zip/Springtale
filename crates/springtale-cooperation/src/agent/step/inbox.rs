//! L3 direct-handoff step (`COOPERATION.md §20.1` + §20.4) — pulls work
//! from the agent's per-substrate inbox sources via `TaskRouter`. Direct
//! assignments preempt the routine scan.
//!
//! Three sources, in priority order:
//! 1. **Direct inbox** (`§20.1`) — `TaskRouter::poll_assigned` reads the
//!    per-agent FIFO populated by `HandoffType::Direct` dispatches, then
//!    falls back to blackboard tasks tagged `assigned_to == agent` from
//!    the CBBA replan path.
//! 2. **FlexibleChain steal** (`§20.4`) — `TaskRouter::try_steal_chain`
//!    consults the per-capability work-stealing pool for chain steps any
//!    capable agent can pick up. The default trait impl returns `None`
//!    so test mocks don't need to wire a flex-chain substrate.
//!
//! EnvironmentMediated handoff (`§20.3`) is consumed by L0 sense via the
//! stigmergy surface store, not here.
//!
//! Trait-bounded per plan §A2 — `&dyn TaskRouter` so the step is mockable
//! independently of the concrete `BlackboardRouter`.

use crate::agent::context::AgentContext;
use crate::agent::result::AgentTickResult;
use crate::authority;
use crate::cadence::ActionDescriptor;
use crate::layer::LayerId;
use crate::routing::trait_::TaskRouter;

pub async fn run(
    router: &dyn TaskRouter,
    ctx: &AgentContext<'_>,
) -> Option<AgentTickResult> {
    if !authority::allows(ctx.momentum.tier, LayerId::L3Direct) {
        return None;
    }
    if let Some(assigned) = router.poll_assigned(ctx.agent_id).await {
        let target = assigned.task.target_connector.name.clone();
        return Some(AgentTickResult {
            agent_id: ctx.agent_id,
            action: Some(ActionDescriptor {
                kind: "direct_handoff".to_owned(),
                target: Some(target),
                payload_hash: 0,
            }),
            alignment: 1.0,
            interference_with: vec![],
            task_claimed: Some(assigned.task),
            task_completed: false,
        });
    }
    let stolen = router
        .try_steal_chain(ctx.capabilities, ctx.agent_id)
        .await?;
    let target = stolen.task.target_connector.name.clone();
    Some(AgentTickResult {
        agent_id: ctx.agent_id,
        action: Some(ActionDescriptor {
            kind: "flex_chain_steal".to_owned(),
            target: Some(target),
            payload_hash: 0,
        }),
        alignment: 1.0,
        interference_with: vec![],
        task_claimed: Some(stolen.task),
        task_completed: false,
    })
}
