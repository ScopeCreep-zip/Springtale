//! Criterion bench #2 — consensus resolution latency.
//!
//! Plan §10.6 line 2: "Consensus resolution latency". Measures the cost
//! of `ConsensusVote::resolve` after N voters have cast ballots. The
//! resolve path iterates ballots to tally, so cost grows with voter
//! count; the plan's latency budget (sub-100 ms at 1000 agents) lets us
//! confirm the tally loop is cheap enough not to dominate tick work.
//!
//! Run with:
//!     cargo bench -p springtale-cooperation --bench consensus_latency

use std::collections::HashMap;

use chrono::Utc;
use criterion::{
    BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};

use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::consensus::{
    ConsensusVote, DecisionDescriptor, VoteChoice,
};

fn build_vote(n: usize) -> ConsensusVote {
    let mut vote = ConsensusVote {
        question: DecisionDescriptor {
            description: "bench".into(),
            options: vec!["yes".into(), "no".into()],
            required_participants: n as u32,
        },
        term: 1,
        ballots: HashMap::new(),
        // Far future — never times out during the bench.
        deadline: Utc::now() + chrono::TimeDelta::hours(1),
        overrides_remaining: HashMap::new(),
        committed: None,
    };
    for i in 0..n {
        let agent = AgentId::new();
        vote.vote(agent, VoteChoice::Option(i % 2));
    }
    vote
}

fn bench_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("consensus_resolve");
    for &n in &[10usize, 100, 1000] {
        let vote = build_vote(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &vote, |b, v| {
            b.iter(|| {
                let res = v.resolve();
                black_box(res);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_resolve);
criterion_main!(benches);
