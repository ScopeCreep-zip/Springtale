//! Criterion bench #1 — cadence bus throughput.
//!
//! Plan §10.6 line 1: "Cadence bus throughput (ticks/sec) at formation
//! sizes 10 / 100 / 1000". Drives the real [`CadenceBus::run`] under a
//! tokio runtime, subscribes N receivers, runs the bus for a fixed
//! wall-clock window, and counts delivered ticks across all subscribers.
//! That is the system's actual throughput — timer + broadcast fan-out
//! combined — not a sync approximation.
//!
//! Per the workspace-standard test/bench convention (see
//! `tests/contract_net_round.rs`, `tests/cbba_replan.rs`), harnesses that
//! panic on unrecoverable setup failure carry `#![allow]` at the top.
//!
//! Run with:
//!     cargo bench -p springtale-cooperation --bench cadence_throughput

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use criterion::{
    BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};

use springtale_cooperation::cadence::CadenceBus;

/// How long each iteration runs the bus. Short enough that the bench
/// completes in criterion's default wall-clock budget, long enough that
/// the 30 Hz bus emits ≥5 ticks even at large N.
const RUN_DURATION: Duration = Duration::from_millis(200);

fn bench_cadence_run(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("cadence_run");
    for &n in &[10usize, 100, 1000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &subs| {
            b.iter(|| {
                rt.block_on(async move {
                    let (bus, _reports_rx) = CadenceBus::default_30hz();
                    let bus = Arc::new(bus);

                    let mut handles = Vec::with_capacity(subs);
                    for _ in 0..subs {
                        let mut rx = bus.subscribe();
                        handles.push(tokio::spawn(async move {
                            let mut n = 0u64;
                            while (rx.recv().await).is_ok() {
                                n += 1;
                            }
                            n
                        }));
                    }

                    let bus_task = {
                        let bus = bus.clone();
                        tokio::spawn(async move { bus.run().await })
                    };

                    tokio::time::sleep(RUN_DURATION).await;
                    bus_task.abort();
                    drop(bus);

                    let mut total = 0u64;
                    for h in handles {
                        if let Ok(n) = h.await {
                            total += n;
                        }
                    }
                    black_box(total);
                });
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_cadence_run);
criterion_main!(benches);
