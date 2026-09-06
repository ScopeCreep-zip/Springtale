//! Build the per-formation `FormationTickResult` for one cadence tick.
//!
//! Sub-passes per `docs/ROADMAP.md §3.2` step 1+1b+2+2b:
//! 1. **L0 surface decay sweep** — drop expired stigmergy surfaces so
//!    the agent step pipeline sees only fresh activity.
//! 2. **The beat** — `agent_pipeline::run`: decide per member against
//!    one snapshot, act together inside `tick.window`, gather in
//!    agent-id order (plan 1.8). Bus state is pre-drained so `react`
//!    never holds the `member_subs` mutex across the loop.
//! 3. **Async report drain** — pulls in `TickReport`s from the
//!    `cadence.reports_sender()` mpsc that arrived between ticks.
//! 4. **Tick processor** — feeds reports + this beat's shared-environment
//!    writes through `tick_processor::process_tick_with_context` so §13
//!    ActionNegation gets a proper Lamport split. This is where members'
//!    simultaneous actions are arbitrated — after the fact, from the log.
//!
//! Member supervision reads `FormationMember::pending` in
//! `tick_steps/supervision.rs`; there is no separate supervisor task.

mod agent_pipeline;
pub(crate) mod executor;
mod fold_interference;
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

    // 2. The beat.
    let reports_sender = deps.cadence.reports_sender();
    let mut member_reports = agent_pipeline::run(
        formation,
        tick,
        deps.bridge,
        deps.sentinel,
        deps.registry,
        deps.store,
        &reports_sender,
        deps.cooperation_tx,
    )
    .await;

    // 3. Async report drain.
    while let Ok(async_report) = deps.cadence_reports_rx.try_recv() {
        member_reports.push(async_report);
    }

    // 4. Tick processor — slice the shared-env write log so ActionNegation
    // gets the right history/records split.
    let snapshot = formation.shared_env.snapshot();
    let cursor = formation
        .last_tick_write_count
        .min(snapshot.write_log.len());
    let action_records = tick_processor::action_records_from_writes(&snapshot.write_log[cursor..]);
    let mut result = tick_processor::process_tick_with_context(
        member_reports,
        action_records,
        &snapshot.write_log[..cursor],
    );
    formation.last_tick_write_count = snapshot.write_log.len();

    // 4b. Fold write-log interference back into the reports so awareness
    // and the mental model see it on the report they store (plan §1.10).
    fold_interference::run(&mut result);

    result
}
