//! Criterion bench #4 — rally cascade detection latency.
//!
//! Plan §10.6 line 4: "Rally cascade detection latency". Measures
//! `rally::cascade::detect_cascade` against formations of 10 / 100 /
//! 1000 agents where a mix of members have low morale. The detector is
//! the hot path inside the tick processor — if this runs slower than
//! one tick at 30 Hz (33 ms), momentum propagation stalls.
//!
//! Run with:
//!     cargo bench -p springtale-cooperation --bench rally_cascade

use std::collections::HashMap;
use std::time::{Duration, Instant};

use criterion::{
    BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};

use springtale_cooperation::awareness::{LocalAwareness, NeighborSnapshot, RoleSignature};
use springtale_cooperation::cadence::{ActionDescriptor, AgentId, TickReport};
use springtale_cooperation::rally::cascade::detect_cascade;
use springtale_cooperation::supervision::Liveness;
use springtale_cooperation::tick_processor::FormationTickResult;
use springtale_cooperation::types::AgentHealth;

fn synth_report(agent: AgentId, alignment: f32) -> TickReport {
    TickReport {
        agent_id: agent,
        tick_sequence: 1,
        action_taken: Some(ActionDescriptor {
            kind: "work".to_owned(),
            target: None,
            payload_hash: 0,
        }),
        latency: Duration::from_millis(5),
        intent_alignment: alignment,
        interference_with: Vec::new(),
    }
}

fn synth_low_morale_awareness() -> LocalAwareness {
    let mut aw = LocalAwareness::default();
    let neighbor = AgentId::new();
    aw.update_neighbor(NeighborSnapshot {
        agent_id: neighbor,
        health: AgentHealth::Incapacitated,
        role: RoleSignature::General,
        fuel_remaining_pct: 0.0,
        last_action_success: false,
        attention_load: 0.0,
        liveness: Liveness::Alive,
        last_updated: Instant::now(),
    });
    aw
}

fn bench_detect(c: &mut Criterion) {
    let mut group = c.benchmark_group("rally_cascade_detect");
    for &n in &[10usize, 100, 1000] {
        // Store owned awareness + ids, then build a fresh borrowed map
        // per iteration (the detector takes `&HashMap<_, &LocalAwareness>`).
        let agents: Vec<AgentId> = (0..n).map(|_| AgentId::new()).collect();
        let awareness_store: Vec<LocalAwareness> = (0..n)
            .map(|i| {
                if i % 3 == 0 {
                    synth_low_morale_awareness()
                } else {
                    LocalAwareness::default()
                }
            })
            .collect();
        let reports: Vec<TickReport> = agents
            .iter()
            .enumerate()
            .map(|(i, a)| synth_report(*a, if i % 4 == 0 { 0.3 } else { 0.9 }))
            .collect();
        let tick_result = FormationTickResult {
            reports,
            interferences: Vec::new(),
            all_succeeded: false,
        };

        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(n),
            &(agents, awareness_store, tick_result),
            |b, (ids, aws, result)| {
                b.iter(|| {
                    let map: HashMap<AgentId, &LocalAwareness> =
                        ids.iter().copied().zip(aws.iter()).collect();
                    let risk = detect_cascade(&map, result);
                    black_box(risk);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_detect);
criterion_main!(benches);
