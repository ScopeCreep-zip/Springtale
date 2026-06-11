//! Embedded scheduler bootstrap — Cron + FsWatcher + JobQueue + event loop
//! wired up in the same process that owns the [`RuntimeState`].
//!
//! Both surfaces of Springtale need this:
//!
//! - **`springtaled`** — the headless daemon. Used to own its own private
//!   `boot/{schedulers,queue,event_loop}.rs` plus an `AppScheduler`
//!   wrapper for its API state. Now delegates here.
//! - **Tauri desktop** — `state.rs::init_runtime` previously initialised
//!   the runtime but never spawned a scheduler ("Desktop connects to
//!   daemon via HTTP"). That left every deployed cron rule un-scheduled
//!   and silently dead. With this module the desktop runs the full
//!   in-process boot, matching `CLAUDE.md`'s "the desktop app IS
//!   springtaled with a GUI" architecture promise.
//!
//! A single source of truth for the wiring keeps both surfaces in step
//! — the daemon and the desktop dispatch identical chains, share the
//! same sentinel, run the same job queue.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};

use springtale_core::rule::Trigger;
use springtale_core::rule::action::Action;
use springtale_core::rule::engine::TriggerEvent;
use springtale_core::rule::types::Rule;
use springtale_scheduler::HeartbeatMonitor;
use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::queue::consumer::JobConsumer;
use springtale_scheduler::queue::producer::JobProducer;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;

use crate::state::RuntimeState;

/// One queued chain — every action of a single rule-match, plus the
/// trigger payload they should resolve `${trigger.*}` against, plus
/// the trigger type so the dispatcher can stamp the right
/// `ExecutionMode` on each fire (Cron / Webhook / etc.). Serialised as
/// the `Job.payload` so the queue is still opaque-JSON-driven.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainJob {
    /// Optional rule id for the executions log. Falls back to a fresh
    /// id when absent — preserves the queue's "anonymous job" case.
    rule_id: Option<String>,
    /// `"Cron"` / `"Webhook"` / `"ConnectorEvent"` / `"FileWatch"` /
    /// `"SystemEvent"` — verbatim from the firing `TriggerEvent`.
    trigger_type: String,
    /// The payload `${trigger.*}` references resolve against.
    trigger_payload: serde_json::Value,
    /// Every action of the rule, in declared order. The dispatcher
    /// runs them as one chain so `${last_*_output.*}` resolves between
    /// steps.
    actions: Vec<Action>,
}

fn trigger_type_to_mode(trigger_type: &str) -> springtale_cooperation::execution::ExecutionMode {
    use springtale_cooperation::execution::ExecutionMode;
    match trigger_type {
        "Cron" => ExecutionMode::Cron,
        "Webhook" => ExecutionMode::Webhook,
        "ConnectorEvent" => ExecutionMode::ConnectorEvent,
        "FileWatch" => ExecutionMode::FileWatch,
        // SystemEvent is a heartbeat-style internal trigger — record
        // it as Manual since the daemon's "Trigger" enum has no
        // dedicated SystemEvent execution mode and Manual is the
        // closest semantic ("scheduled by the daemon itself").
        _ => ExecutionMode::Manual,
    }
}

/// Owned handle the caller stores so it can register or cancel a
/// rule's trigger after the initial bootstrap (e.g. when a new
/// recipe deploys at runtime).
///
/// `cron` and `fs_watcher` mutex-guards mirror what was previously
/// `springtaled::scheduler::AppScheduler` — the daemon now imports
/// this type instead.
#[derive(Clone)]
pub struct EmbeddedScheduler {
    pub cron: Arc<Mutex<CronExecutor>>,
    pub fs_watcher: Arc<Mutex<FsWatcher>>,
    pub trigger_tx: mpsc::Sender<TriggerEvent>,
    pub producer: Arc<JobProducer>,
}

impl EmbeddedScheduler {
    /// Schedule a rule's trigger (cron or filesystem watch). Other
    /// trigger types (webhook / connector event / system event) need
    /// no per-rule scheduler registration — they're driven by the
    /// gateways that own those event streams.
    pub async fn schedule(&self, rule: &Rule) -> Result<(), String> {
        match &rule.trigger {
            Trigger::Cron { expression } => {
                let mut cron = self.cron.lock().await;
                cron.schedule(&rule.name, expression)
                    .map_err(|e| format!("failed to schedule cron trigger: {e}"))
            }
            Trigger::FileWatch { path, .. } => {
                let mut watcher = self.fs_watcher.lock().await;
                watcher
                    .watch(path)
                    .map_err(|e| format!("failed to watch path: {e}"))
            }
            _ => Ok(()),
        }
    }

    /// Cancel a previously-registered trigger. Idempotent —
    /// unscheduling something that was never scheduled is a no-op.
    pub async fn unschedule(&self, rule: &Rule) {
        match &rule.trigger {
            Trigger::Cron { .. } => {
                let mut cron = self.cron.lock().await;
                if cron.cancel(&rule.name) {
                    tracing::info!(rule = %rule.name, "cancelled cron trigger");
                }
            }
            Trigger::FileWatch { path, .. } => {
                let mut watcher = self.fs_watcher.lock().await;
                if let Err(e) = watcher.unwatch(path) {
                    tracing::warn!(rule = %rule.name, error = %e, "failed to unwatch path");
                }
            }
            _ => {}
        }
    }
}

/// What [`bootstrap`] returns. The caller is expected to retain
/// `scheduler` for the lifetime of the runtime (it owns the cron +
/// fs_watcher handles), keep `heartbeat_monitor` alive if heartbeat
/// is in use, and `tokio::spawn` ownership of the spawned event-loop
/// task is internalised — drop the returned scheduler and the loop
/// terminates naturally when its `trigger_rx` closes.
pub struct EmbeddedBootHandle {
    pub scheduler: EmbeddedScheduler,
    pub heartbeat_monitor: Arc<Mutex<HeartbeatMonitor>>,
}

/// Spawn the in-process scheduler + job queue + trigger event loop
/// around the supplied `RuntimeState`.
///
/// Pre-schedules every cron and file-watch rule already in the store
/// so a fresh boot picks up everything the previous session deployed.
///
/// Background tasks spawned:
///   1. `CronExecutor` per scheduled rule (one task each)
///   2. `FsWatcher` (single watcher, multiple paths)
///   3. `HeartbeatMonitor` (only if `heartbeat_interval_secs > 0`)
///   4. `JobConsumer::run` — drains the job queue, dispatches each
///      action through the runtime's `capability_bridge` + `sentinel`
///   5. event loop — `trigger_rx → dispatch_event → producer.enqueue`
pub async fn bootstrap(
    runtime: &RuntimeState,
    heartbeat_interval_secs: u64,
) -> Result<EmbeddedBootHandle, String> {
    let (trigger_tx, trigger_rx) = mpsc::channel::<TriggerEvent>(256);

    let mut cron_executor = CronExecutor::new(trigger_tx.clone());
    let mut fs_watcher = FsWatcher::new(trigger_tx.clone())
        .map_err(|e| format!("failed to create filesystem watcher: {e}"))?;

    // Pre-schedule existing rules so a cold restart picks up
    // everything the previous session deployed.
    let rules = runtime
        .store
        .list_rules()
        .await
        .map_err(|e| format!("failed to load rules for scheduler: {e}"))?;
    for rule in &rules {
        match &rule.trigger {
            Trigger::Cron { expression, .. } => {
                if let Err(e) = cron_executor.schedule(&rule.name, expression) {
                    tracing::warn!(
                        rule = %rule.name,
                        error = %e,
                        "failed to schedule cron trigger on boot",
                    );
                }
            }
            Trigger::FileWatch { path, .. } => {
                if let Err(e) = fs_watcher.watch(path) {
                    tracing::warn!(
                        rule = %rule.name,
                        error = %e,
                        "failed to watch path on boot",
                    );
                }
            }
            _ => {}
        }
    }

    let mut heartbeat_monitor = HeartbeatMonitor::new(heartbeat_interval_secs, trigger_tx.clone());
    if heartbeat_interval_secs > 0 {
        heartbeat_monitor.start();
        tracing::info!(
            interval_secs = heartbeat_interval_secs,
            "heartbeat monitor started",
        );
    }

    tracing::info!(
        cron_jobs = cron_executor.list().len(),
        watched_paths = fs_watcher.watched_paths().len(),
        "scheduler started",
    );

    // ── Job queue (consumer dispatches via capability_bridge + sentinel)
    //
    // CRITICAL: each queued job carries an ENTIRE rule chain, not a
    // single action. Recipes that thread state between steps (e.g.
    // `scheduled-web-fetch` does HTTP get → JSONPath extract → Telegram
    // send and the send references `${last_extract_output.value}`) only
    // work when the actions share one `ChainContext`. Enqueuing one
    // action per job dispatches each in isolation with a fresh chain,
    // dropping every `${last_*_output.*}` reference on the floor.
    let (job_tx, job_rx) = mpsc::channel(100);
    let producer = Arc::new(JobProducer::new(job_tx));
    let mut consumer = JobConsumer::new(job_rx, 4);

    let dispatch_bridge = runtime.capability_bridge.clone();
    let dispatch_sentinel = runtime.sentinel.clone();
    let dispatch_notify = runtime.notification_tx.clone();
    consumer.set_handler(Arc::new(move |job| {
        let bridge = dispatch_bridge.clone();
        let sent = dispatch_sentinel.clone();
        let notify_tx = dispatch_notify.clone();
        Box::pin(async move {
            let chain_job: ChainJob = serde_json::from_value(job.payload)
                .map_err(|e| format!("failed to deserialize chain job: {e}"))?;

            let mode = trigger_type_to_mode(&chain_job.trigger_type);
            let rule_id = chain_job
                .rule_id
                .as_deref()
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
                .map(springtale_core::rule::RuleId)
                .unwrap_or_default();
            let exec =
                springtale_cooperation::execution::ExecutionContext::for_global(rule_id, mode);

            let chain = crate::dispatch::dispatch_actions(
                &chain_job.actions,
                &bridge,
                &sent,
                exec,
                chain_job.trigger_payload,
            )
            .await
            .map_err(|e| e.to_string())?;

            // Delivery: fan out every user-facing Notify/SendMessage
            // step to the chat stream + OS notification. A send error
            // means no subscriber is currently attached (e.g. desktop
            // chat panel closed) — best-effort, so we trace and move
            // on rather than fail the job.
            for event in crate::notification::NotificationEvent::from_chain(&chain) {
                if let Err(e) = notify_tx.send(event) {
                    tracing::trace!(error = %e, "no notification subscribers — delivery dropped");
                }
            }
            Ok(())
        })
    }));
    tokio::spawn(async move {
        if let Err(e) = consumer.run().await {
            tracing::error!(error = %e, "job consumer error");
        }
    });
    tracing::info!("job queue started (concurrency: 4)");

    // ── Trigger event loop: drains trigger_rx, matches rules, enqueues
    // one job PER MATCHING RULE (not per action) so the action chain
    // executes with shared `ChainContext` and `${last_*_output.*}`
    // references resolve correctly.
    let engine = runtime.engine.clone();
    let normalize_registry = runtime.registry.clone();
    let event_producer = producer.clone();
    tokio::spawn(async move {
        let mut rx = trigger_rx;
        while let Some(mut event) = rx.recv().await {
            // Anti-corruption boundary: every ConnectorEvent entering the
            // rule engine — from the webhook ingress OR a polling gateway,
            // both of which feed this one `trigger_rx` — is normalized to
            // the connector's declared flat trigger schema HERE, the single
            // chokepoint. Recipes therefore resolve `${trigger.*}` against
            // canonical fields (e.g. GitHub `pusher` → a username) rather
            // than a raw nested provider blob or a missing-field placeholder.
            if event.trigger_type == "ConnectorEvent"
                && let Some(connector_name) = event.connector.clone()
            {
                let reg = normalize_registry.read().await;
                if let Some(entry) = reg.get(&connector_name) {
                    let trigger = event.event.clone().unwrap_or_default();
                    event.payload = entry.host.normalize_event(&trigger, event.payload);
                }
            }
            let engine = engine.read().await;
            let matches = springtale_core::router::dispatch::dispatch_event(&engine, &event);
            for rule_match in &matches {
                tracing::info!(
                    rule = %rule_match.rule_name,
                    actions = rule_match.actions.len(),
                    "rule matched trigger — enqueuing chain",
                );
                let chain_job = ChainJob {
                    rule_id: Some(rule_match.rule_id.0.to_string()),
                    trigger_type: event.trigger_type.clone(),
                    trigger_payload: event.payload.clone(),
                    actions: rule_match.actions.iter().cloned().collect(),
                };
                let payload = match serde_json::to_value(&chain_job) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!(
                            rule = %rule_match.rule_name,
                            error = %e,
                            "failed to serialize chain job",
                        );
                        continue;
                    }
                };
                if let Err(e) = event_producer.enqueue(payload, 3).await {
                    tracing::error!(
                        rule = %rule_match.rule_name,
                        error = %e,
                        "failed to enqueue chain job",
                    );
                }
            }
        }
        tracing::info!("trigger event loop terminated");
    });

    // Wire ConnectorEvent handlers for every enabled ConnectorEvent rule
    // and publish the registry on RuntimeState so every deploy surface
    // (recipe apply, chat deployer, rule CRUD) attaches/detaches through
    // the SAME instance. Both the daemon and desktop call bootstrap, so
    // this is the single place connector-event triggers come up.
    let registry = crate::triggers::wire_connector_events(
        &runtime.registry,
        &runtime.engine,
        trigger_tx.clone(),
        runtime.store.clone(),
    )
    .await;
    if runtime.trigger_registry.set(registry).is_err() {
        tracing::warn!("trigger_registry already initialised — bootstrap ran twice?");
    }

    let scheduler = EmbeddedScheduler {
        cron: Arc::new(Mutex::new(cron_executor)),
        fs_watcher: Arc::new(Mutex::new(fs_watcher)),
        trigger_tx,
        producer,
    };

    Ok(EmbeddedBootHandle {
        scheduler,
        heartbeat_monitor: Arc::new(Mutex::new(heartbeat_monitor)),
    })
}
