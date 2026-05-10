//! L1 scan step — pull the highest-priority capability-matched task via
//! `TaskRouter` (`COOPERATION.md §20.2` Hayes-Roth blackboard pattern).
//!
//! Trait-bounded per plan §A2 — `&dyn TaskRouter` so any router impl
//! plugs in (`BlackboardRouter` in production, mocks in tests). The
//! router enforces tier authority + capability filtering internally.

use crate::agent::context::AgentContext;
use crate::agent::result::AgentTickResult;
use crate::cadence::ActionDescriptor;
use crate::routing::trait_::TaskRouter;

pub async fn run(
    router: &dyn TaskRouter,
    ctx: &AgentContext<'_>,
) -> Option<AgentTickResult> {
    // Tier authority enforced inside `TaskRouter::scan` — Cold-tier
    // routers return None per §7 capability table.
    //
    // B5 priority merge: pass awareness so Warming+ routers can weight
    // priority by neighbor recent-success (`COOPERATION.md §8` Total War
    // morale-via-proximity). Cold tier returns before awareness is read.
    let pick = router
        .scan(ctx.capabilities, ctx.momentum.tier, Some(ctx.awareness))
        .await?;
    let target = pick.task.target_connector.name.clone();
    Some(AgentTickResult {
        agent_id: ctx.agent_id,
        action: Some(ActionDescriptor {
            kind: "task_claimed".to_owned(),
            target: Some(target),
            payload_hash: 0,
        }),
        alignment: 1.0,
        interference_with: vec![],
        task_claimed: Some(pick.task),
        task_completed: false,
    })
}
