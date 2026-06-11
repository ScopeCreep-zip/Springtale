//! Build the per-formation `FormationTickResult` for one cadence tick.
//!
//! Five sub-passes per `docs/ROADMAP.md §3.2` step 1+1b+2+2b:
//! 1. **L0 surface decay sweep** — drop expired stigmergy surfaces so
//!    the agent step pipeline sees only fresh activity.
//! 2. **Bus state pre-drain** — collect each member's queued state
//!    broadcasts so `react::run` can fold them into awareness without
//!    holding the `member_subs` mutex across the per-member loop body.
//! 3. **Per-member agent loop** — `agent_pipeline::run` composes the four
//!    `agent::step::*` files (sense → inbox → react → scan) and
//!    `executor::execute` runs the chosen task through the
//!    claim → dispatch → deposit pipeline gated by autonomy + pacing +
//!    consensus policy.
//! 4. **Async report drain** — pulls in `TickReport`s from the
//!    `cadence.reports_sender()` mpsc that members completed between
//!    cadence ticks (out-of-band reporting).
//! 5. **Tick processor** — feeds reports + new shared-environment writes
//!    through `tick_processor::process_tick_with_context` so §13
//!    ActionNegation gets a proper Lamport split.
//! 6. **Rally supervisor drain** — non-blocking pull of every completed
//!    member task; supervision runs on the same cadence as the rest of
//!    the formation (`COOPERATION.md §15.2`).
//!
//! There is no `decide_agent_tick` indirection here — `AgentLoop::tick`
//! plus the `executor` module own the full per-member decision +
//! execution path (forward refactor; the legacy `agent_loop` module was
//! deleted with the directory restructure).

mod agent_pipeline;
mod executor;
mod state_drain;

use springtale_cooperation::cadence::Tick;
use springtale_cooperation::tick_processor::{self, FormationTickResult};

use crate::cooperation::formation::Formation;
use crate::runtime::tick_steps::TickDeps;

pub async fn run(
    formation: &mut Formation,
    tick: &Tick,
    deps: &mut TickDeps<'_>,
) -> FormationTickResult {
    // 1. L0 stigmergy decay sweep — drop surfaces whose TTL elapsed so
    // agents sense only fresh activity. Runs at the head of the tick so
    // every step downstream operates on the post-decay set.
    formation.surfaces.decay(std::time::Instant::now());

    // 2 + 3. Per-member loop with pre-drained state messages.
    let reports_sender = deps.cadence.reports_sender();
    let mut member_reports = agent_pipeline::run(
        formation,
        tick,
        deps.bridge,
        deps.sentinel,
        deps.store,
        &reports_sender,
        deps.cooperation_tx,
    )
    .await;

    // 4. Async report drain.
    while let Ok(async_report) = deps.cadence_reports_rx.try_recv() {
        member_reports.push(async_report);
    }

    // 5. Tick processor — slice the shared-env write log so ActionNegation
    // gets the right history/records split.
    let snapshot = formation.shared_env.snapshot();
    let cursor = formation
        .last_tick_write_count
        .min(snapshot.write_log.len());
    let action_records = tick_processor::action_records_from_writes(&snapshot.write_log[cursor..]);
    let result = tick_processor::process_tick_with_context(
        member_reports,
        action_records,
        &snapshot.write_log[..cursor],
    );
    formation.last_tick_write_count = snapshot.write_log.len();

    // 6. Rally supervisor drain.
    let drained = springtale_cooperation::rally::supervise::drain(&mut formation.rally);
    if drained > 0 {
        tracing::debug!(
            formation_id = %formation.id.0,
            drained,
            "rally supervisor drained member outcomes"
        );
    }

    result
}
