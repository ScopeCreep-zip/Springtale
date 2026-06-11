//! Criterion bench #3 — environment RCU write contention.
//!
//! Plan §10.6 line 3: "Environment RCU write contention". Plan §16.8:
//! "sub-100 ms coordination latency at 1000 agents". We measure N total
//! writes distributed across a fixed worker pool (num_cpus threads) so
//! the measurement reflects ArcSwap contention, not per-write thread
//! creation. The previous variant spawned one OS thread per write,
//! which hit OS scheduler overhead at N=1000 and reported artificially
//! high latencies unrelated to the RCU cost.
//!
//! The workspace-standard test/bench convention (see
//! `tests/contract_net_round.rs`) carries `#![allow]` for setup
//! patterns that panic on unrecoverable failure. Workers here panic on
//! thread-scope errors, which is consistent with that convention.
//!
//! Run with:
//!     cargo bench -p springtale-cooperation --bench environment_rcu

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use serde_json::Value;

use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::state::shared_env::SharedEnvironment;

fn worker_count() -> usize {
    // Fixed small pool — the plan's interest is ArcSwap contention, not
    // parallelism. 8 workers is enough to surface per-write retry cost
    // from concurrent RCU rebuilds without saturating the scheduler.
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4)
}

fn bench_concurrent_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("environment_rcu");
    let workers = worker_count();
    for &n in &[10usize, 100, 1000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &total_writes| {
            b.iter(|| {
                let env = Arc::new(SharedEnvironment::new());
                let next = Arc::new(AtomicUsize::new(0));
                std::thread::scope(|scope| {
                    for _ in 0..workers {
                        let env = env.clone();
                        let next = next.clone();
                        scope.spawn(move || {
                            let agent = AgentId::new();
                            loop {
                                let i = next.fetch_add(1, Ordering::Relaxed);
                                if i >= total_writes {
                                    break;
                                }
                                env.write(&format!("writer-{i}"), Value::from(i as u64), agent);
                            }
                        });
                    }
                });
                black_box(env.version());
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_concurrent_writes);
criterion_main!(benches);
