//! apps/springtale-cli/examples/task-runner.rs
//!
//! Worked example §7.1 from `docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md`.
//! Demonstrates the plan's Checkpoint 1 target: a headless CLI that
//! spawns a formation of worker agents, runs ticks, aggregates reports,
//! and prints the result. No AI adapter — pure cooperation-module exercise.
//!
//! Usage (run from the workspace root):
//!
//!     cargo run -p springtale-cli --example task-runner -- "summarize /tmp"
//!     cargo run -p springtale-cli --example task-runner -- --workers 5 "<task>"
//!
//! The plan's §7.1 snippet was written against a hypothetical simpler
//! `Formation::new(id, ctx, bus, cap)` API; the baseline uses the
//! richer `Formation::new(members, intent, constraints, deps)` + a
//! `new_disconnected` constructor for tooling. This example uses the
//! real baseline API while preserving the plan's demo goals:
//!
//! - Cadence broadcast (multiple agents receiving ticks)
//! - Formation join (members list initialized at construction)
//! - Tick report fan-in on the cadence reports channel
//! - Graceful shutdown when max_ticks reached or workers done

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;

use springtale_bot::cooperation::formation::{Formation, FormationDeps, FormationMember};
use springtale_cooperation::TaskDescriptor;
use springtale_cooperation::awareness::InMemoryGossipStore;
use springtale_cooperation::cadence::{ActionDescriptor, AgentId, CadenceBus, TickReport};
use springtale_cooperation::handoff::FlexibleChainPool;
use springtale_cooperation::types::FormationConstraints;
use springtale_cooperation::{IntentPattern, StabilizeReason};
use springtale_store::backend::InMemoryBackend;

#[derive(Parser, Debug)]
#[command(
    name = "task-runner",
    about = "Minimal cooperation-module demo (plan §7.1)"
)]
struct Args {
    /// Free-text task description — worker agents echo this back as their action.
    task: String,

    /// How many worker agents to spawn.
    #[arg(short, long, default_value_t = 3)]
    workers: usize,

    /// Maximum ticks to wait before concluding.
    #[arg(long, default_value_t = 60)]
    max_ticks: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("springtale=info,cooperation=info")
        .init();
    let args = Args::parse();

    // 1. Build cadence bus at the Springtale default (30 Hz, 256-tick backlog).
    //    `default_30hz` returns the bus plus the reports channel receiver;
    //    we hold the receiver so the coordinator can drain reports.
    let (bus, mut reports_rx) = CadenceBus::default_30hz();
    let bus = Arc::new(bus);

    // 2. Build `FormationDeps` pointing at the cadence bus we just made,
    //    plus an in-memory store + gossip + flex-chain pool (same shape
    //    `Formation::new_disconnected` uses, but wiring them explicitly
    //    so the example is honest about what each dep is for).
    let deps = FormationDeps {
        cadence: bus.clone(),
        store: Arc::new(InMemoryBackend::new()),
        gossip_store: Arc::new(InMemoryGossipStore::new()),
        flex_chain_pool: Arc::new(FlexibleChainPool::new()),
        formation_gossip: None,
    };

    // 3. Build the workers. Each member declares `text.read` +
    //    `text.summarize` capabilities so the blackboard can route work.
    let workers: Vec<FormationMember> = (0..args.workers)
        .map(|_| {
            FormationMember::new(
                AgentId::new(),
                vec!["text.read".into(), "text.summarize".into()],
            )
        })
        .collect();
    let worker_ids: Vec<AgentId> = workers.iter().map(|m| m.agent_id).collect();

    // 4. Assemble the formation. `Formation::new` returns the formation
    //    plus two dispatcher ends; for this one-shot example we drop
    //    both (no cross-agent protocol / ack routing needed).
    let (formation, _proto, _ack) = Formation::new(
        workers,
        IntentPattern::Reconnoiter {
            target: TaskDescriptor::from(args.task.clone()),
        },
        FormationConstraints::default(),
        deps,
    );
    let formation = Arc::new(formation);
    tracing::info!(
        formation_id = %formation.id,
        workers = worker_ids.len(),
        "formation assembled — Cold tier"
    );

    // 5. Spawn a task per worker. Each subscribes to the cadence bus and
    //    emits a TickReport on every tick. Tasks exit when the bus's
    //    senders drop (i.e. `bus_task.abort()` at the end of main).
    let mut worker_handles = Vec::new();
    for agent_id in worker_ids.iter().copied() {
        let reports_tx = bus.reports_sender();
        let task_text = args.task.clone();
        let bus = bus.clone();
        worker_handles.push(tokio::spawn(async move {
            let mut tick_rx = bus.subscribe();
            loop {
                match tick_rx.recv().await {
                    Ok(tick) => {
                        let report = TickReport {
                            agent_id,
                            tick_sequence: tick.sequence,
                            action_taken: Some(ActionDescriptor {
                                kind: "summarize".to_owned(),
                                target: Some(task_text.clone()),
                                payload_hash: tick.sequence.0,
                            }),
                            latency: Duration::from_millis(5),
                            intent_alignment: 1.0,
                            interference_with: Vec::new(),
                        };
                        let _ = reports_tx.send(report).await;
                    }
                    Err(_) => return,
                }
            }
        }));
    }

    // 6. Spawn the cadence bus driver. Holding the `Arc<CadenceBus>` in
    //    the main task keeps the broadcast channel open.
    let bus_task = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.run().await })
    };

    // 7. Drain reports up to `max_ticks` or until every worker has
    //    reported at least `successful_threshold` times. Successful =
    //    `intent_alignment > 0.5` (matches `all_succeeded` in the tick
    //    processor).
    let successful_threshold: u32 = 3;
    let mut success_count = 0u32;
    let mut aggregated = Vec::<String>::new();
    let mut last_tick = springtale_cooperation::TickId::ZERO;

    while let Some(report) = reports_rx.recv().await {
        last_tick = report.tick_sequence;
        if report.tick_sequence.0 > args.max_ticks {
            break;
        }
        if report.intent_alignment > 0.5 {
            success_count += 1;
        }
        if let Some(action) = &report.action_taken {
            aggregated.push(format!(
                "tick {} agent {} -> {} ({})",
                report.tick_sequence,
                report.agent_id,
                action.kind,
                action.target.as_deref().unwrap_or("-")
            ));
        }
        // Stop once every worker has contributed at least `threshold` reports.
        if success_count >= (args.workers as u32 * successful_threshold) {
            break;
        }
    }

    // 8. Wind down. Abort the bus + worker tasks and print summary.
    bus_task.abort();
    for h in worker_handles {
        h.abort();
    }

    println!("\n── task-runner summary ──");
    println!("  task              : {}", args.task);
    println!("  workers           : {}", args.workers);
    println!("  ticks elapsed     : {}", last_tick);
    println!("  successful reports: {}", success_count);
    println!("  intent            : Reconnoiter (pure observation at Cold tier)");
    println!("  formation id      : {}", formation.id);
    println!("\n── report sample ──");
    for line in aggregated.iter().take(6) {
        println!("  {line}");
    }
    if aggregated.len() > 6 {
        println!("  ... ({} more)", aggregated.len() - 6);
    }

    // Silence unused import warning — `StabilizeReason` is exported here
    // so the example file doubles as a reference for the canonical
    // IntentPattern variants even when Reconnoiter is the one exercised.
    let _ = std::any::type_name::<StabilizeReason>();
    Ok(())
}
