//! Per-tick pipeline composition. Each step is one named module so it can
//! be unit-tested with mocked deps. Replaces the monolithic match arms in
//! the old `event_loop.rs::handle_cadence_tick` (was 1462 lines; per
//! `pure-noodling-biscuit.md` lines 1843–1849 the target is <150).
//!
//! Step order matches `docs/ROADMAP.md §3.2` (the canonical 14-step
//! pipeline). Steps are added incrementally — each B-series task moves
//! one piece of logic out of `event_loop.rs` into its own file here.

use std::sync::Arc;

use tokio::sync::mpsc;

use springtale_cooperation::cadence::{CadenceBus, TickReport};
use springtale_core::canvas::CanvasUpdate;
use tokio::sync::broadcast;

use crate::orchestrator::intervention::{
    action::DefaultInterventionAction, evaluator::RuleBasedEvaluator,
};
use crate::runtime::lifecycle::Bot;

pub mod build_reports;
pub mod check_cascade;
pub mod check_interventions;
pub mod check_pacing;
pub mod emit_canvas_update;
pub mod expire_commits;
pub mod fuel;
pub mod gossip_awareness;
pub mod handle_command;
pub mod implicit_signals;
pub mod liveness;
pub mod log_interference;
pub mod orchestrate_step;
pub mod persist_momentum;
pub mod publish_context;
pub mod publish_formation_view;
pub mod recovery;
pub mod replan_cbba;
pub mod resolve_consensus;
pub mod shutdown;
pub mod state_broadcast;
pub mod supervision;
pub mod tail;
pub mod tick_commits;
pub mod transformation;
pub mod update_mental_model;
pub mod update_momentum;

pub use handle_command::handle_formation_command;
pub use shutdown::log_shutdown_snapshot;

use crate::cooperation::cadence::Tick;
use crate::cooperation::formation::Formation;

/// One formation's full 14-step pipeline. Order matches `docs/ROADMAP.md
/// §3.2`. Each step is one named module under `tick_steps/`.
pub async fn run_tick(formation: &mut Formation, tick: &Tick, deps: &mut TickDeps<'_>) {
    // §22 frequency modulation — the pacing divider skips bus ticks so a
    // formation's effective tick rate tracks its phase (Peak ÷1 … Recovery
    // ÷6). Skipping is safe: momentum decay, commit deadlines, and vote
    // deadlines are all wall-clock based and settle on the next processed
    // tick.
    if !tick
        .sequence
        .0
        .is_multiple_of(formation.pacing.tick_divider())
    {
        return;
    }
    formation.current_tick = tick.sequence;
    // True per-formation elapsed time since the last PROCESSED tick —
    // pacing timers run on this, not on `tick.window` (the agent commit
    // window) and not on the bus interval (wrong under the divider).
    let elapsed = formation
        .last_tick_at
        .map(|prev| tick.timestamp.duration_since(prev))
        .unwrap_or_default();
    formation.last_tick_at = Some(tick.timestamp);

    let result = build_reports::run(formation, tick, deps).await;
    formation.momentum.check_decay();
    update_momentum::run(formation, &result, deps.cooperation_tx);
    liveness::run(formation, tick, &result);
    supervision::run(formation, &result, deps.cooperation_tx);
    fuel::run(formation);
    implicit_signals::run(formation, &result);
    state_broadcast::run(formation, &result);
    persist_momentum::run(deps.store.as_ref(), formation).await;
    publish_context::run(formation);
    gossip_awareness::run(formation, &result).await;
    log_interference::run(formation, &result, deps.cooperation_tx);
    check_pacing::run(formation, &result, elapsed, deps.cooperation_tx);
    check_cascade::run(formation, &result, deps.store.as_ref(), deps.cooperation_tx).await;
    check_interventions::run(formation, deps).await;
    recovery::run(formation, deps.cooperation_tx);
    transformation::run(formation, deps.role_registry.as_ref(), deps.cooperation_tx);
    replan_cbba::run(formation, deps.cooperation_tx);
    resolve_consensus::run(formation, deps.cooperation_tx);
    tick_commits::run(formation, deps.cooperation_tx);
    expire_commits::run(formation, deps.cooperation_tx);
    update_mental_model::run(formation, &result);
    orchestrate_step::run(formation, deps.registry).await;
    // G6 — broadcast this formation's view on the cross-formation
    // gossip bus. No-op when `formation_gossip` is None (CLI / test).
    publish_formation_view::run(formation);
    // F4 — emit per-tick canvas summary so subscribers (web SSE, desktop
    // Tauri Channel) receive live formation state.
    if let Some(tx) = deps.canvas_tx {
        emit_canvas_update::run(formation, tx);
    }
}

/// Dependencies threaded through the per-tick pipeline.
///
/// Borrowed from the `Bot` for the duration of one tick. Each step takes
/// `&mut Formation` (or `&Formation`) plus the slice of `TickDeps` it needs.
/// Steps don't see the whole `Bot` so they can be unit-tested with hand-built
/// `TickDeps`. The `'a` lifetime keeps the borrow tied to the bot reference
/// so we don't hold cross-tick state.
pub struct TickDeps<'a> {
    pub bridge: &'a springtale_runtime::CapabilityBridge,
    pub sentinel: &'a Arc<springtale_sentinel::Sentinel>,
    pub store: &'a Arc<dyn springtale_store::StorageBackend>,
    pub registry:
        &'a Arc<tokio::sync::RwLock<springtale_connector::registry::store::ConnectorRegistry>>,
    pub role_registry: &'a Arc<springtale_cooperation::role::RoleRegistry>,
    pub cadence: &'a Arc<CadenceBus>,
    pub cadence_reports_rx: &'a mut mpsc::Receiver<TickReport>,
    /// L6 commander-override pure evaluator (`COOPERATION.md §3.4`).
    pub intervention_evaluator: &'a RuleBasedEvaluator,
    /// L6 commander-override executor.
    pub intervention_action: &'a DefaultInterventionAction,
    /// F4: shared `runtime.canvas_tx` broadcast — `emit_canvas_update::run`
    /// publishes per-tick formation summaries here so both the web SSE
    /// (`/canvas/stream`) and the desktop Tauri Channel<CanvasUpdate>
    /// (subscribe_canvas) receive live state. `None` in headless / test
    /// builds — the step short-circuits when no sender is wired.
    pub canvas_tx: Option<&'a broadcast::Sender<CanvasUpdate>>,
    /// Phase H2: shared `runtime.cooperation_tx` broadcast — every
    /// internal-state cooperation event (intervention fired, sacrifice
    /// yielded, vote opened, role transformed, member marked down,
    /// supervisor escalation, pacing phase change, cascade hit, recovery
    /// action, surface deposit, interference event, CFP/replan/commit
    /// outcome) publishes a `CooperationEventEnvelope` here. Subscribers:
    /// `/cooperation/events` SSE (web) and `subscribe_cooperation` IPC
    /// channel (desktop). `None` in headless/test builds — emit sites
    /// short-circuit when no sender is wired (mirrors `canvas_tx`).
    pub cooperation_tx:
        Option<&'a broadcast::Sender<springtale_cooperation::CooperationEventEnvelope>>,
}

/// Borrow a `TickDeps` from the `Bot` for one tick. Lifetimes keep the
/// borrow tied to the bot reference so steps can't keep state across ticks.
pub fn deps_from_bot(bot: &mut Bot) -> TickDeps<'_> {
    TickDeps {
        bridge: &bot.capability_bridge,
        sentinel: &bot.sentinel,
        store: &bot.store,
        registry: &bot.registry,
        role_registry: &bot.role_registry,
        cadence: &bot.cadence,
        cadence_reports_rx: &mut bot.cadence_reports_rx,
        intervention_evaluator: &bot.intervention_evaluator,
        intervention_action: &bot.intervention_action,
        canvas_tx: bot.canvas_tx.as_ref(),
        cooperation_tx: bot.cooperation_tx.as_ref(),
    }
}
