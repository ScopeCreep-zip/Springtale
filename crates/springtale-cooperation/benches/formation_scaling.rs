//! Criterion benches for formation coordination scaling
//! (COOPERATION_IMPLEMENTATION_PLAN.md §16.8).
//!
//! Success criteria per the plan:
//! - Linear scaling up to 100 agents.
//! - Sub-100 ms coordination latency at 1000 agents.
//!
//! The benches here exercise `tick_processor::process_tick`, which is
//! the per-tick hot path the bot's event loop drives. It's pure — no
//! I/O, no async — so a single-threaded criterion harness is enough.
//! The interference detector is O(N²) pairwise on reports, so the
//! curve we expect at N = 10 / 100 / 1000 is *roughly* quadratic in N;
//! the plan's "linear at 100" bound means ≤ ~1 ms at N = 100 for the
//! tick to be real-time at 30 Hz.
//!
//! Run with:
//!     cargo bench -p springtale-cooperation

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use springtale_cooperation::cadence::{ActionDescriptor, AgentId, TickReport};
use springtale_cooperation::tick_processor;

fn synthetic_report(i: usize, tick: u64) -> TickReport {
    TickReport {
        agent_id: AgentId::new(),
        tick_sequence: springtale_cooperation::TickId(tick),
        action_taken: Some(ActionDescriptor {
            // Targeting a small set of keys so the detector finds some
            // interference pairs — otherwise the O(N²) loop runs but
            // the match rate is zero and we'd be measuring the wrong
            // branch of the detector.
            kind: "send".to_owned(),
            target: Some(format!("chat-{}", i % 8)),
            payload_hash: i as u64,
        }),
        latency: Duration::from_millis(5),
        intent_alignment: 0.95,
        interference_with: Vec::new(),
    }
}

fn bench_process_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("process_tick");
    for &n in &[10usize, 100, 1000] {
        let reports: Vec<TickReport> = (0..n).map(|i| synthetic_report(i, 1)).collect();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &reports, |b, input| {
            b.iter(|| {
                let result = tick_processor::process_tick(black_box(input.clone()));
                black_box(result);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_process_tick);
criterion_main!(benches);
