# Cooperation benchmarks

Tracks the five criterion benches that gate the §16.8 ship criterion:

> Linear scaling ≤100 agents, sub-100ms coordination latency at 1000 agents.

The benches live under `crates/springtale-cooperation/benches/` and are
declared in that crate's `Cargo.toml` so `cargo bench
-p springtale-cooperation` runs them all.

## The five benches

| Bench | Plan §10.6 line | What it measures | §16.8 budget |
| ----- | --------------- | ---------------- | ------------ |
| `formation_scaling` | tick processor | `tick_processor::process_tick` wall time at N=10/100/1000 | ≤ ~1 ms at N=100 (real-time at 30 Hz) |
| `cadence_throughput` | cadence bus | ticks/sec delivered through `CadenceBus::run` with N subscribers | linear in N |
| `consensus_latency` | consensus vote | `ConsensusVote::resolve` tally cost across N ballots | sub-100 ms at N=1000 |
| `environment_rcu` | shared environment | RCU write contention through `SharedEnvironment` with worker pool ≈ `num_cpus` | linear in writers |
| `rally_cascade` | rally cascade | `rally::cascade::detect_cascade` over N-agent awareness graph | < 33 ms (one tick at 30 Hz) |

Each bench parameterises N over `{10, 100, 1000}` so the linear-vs-quadratic
shape is visible from the criterion summary, not just a single point.

## Running

```bash
# All five benches, default 100-sample runs:
cargo bench -p springtale-cooperation

# Single bench:
cargo bench -p springtale-cooperation --bench formation_scaling

# Save the JSON output for tracking:
cargo bench -p springtale-cooperation -- --output-format=verbose
# Per-bench reports land under target/criterion/<bench>/<param>/.
```

## What "passing" looks like

The ship criterion is *operational*, not absolute: each measurement
must be small enough that the surrounding event loop hits its 30 Hz
budget (33 ms / tick). The targets are:

- **formation_scaling/process_tick/100** — ≤ ~1 ms (per the file's own
  doc comment). Above ~5 ms, the tick loop starves at 30 Hz and the
  interference detector becomes the bottleneck.
- **formation_scaling/process_tick/1000** — ≤ 100 ms (the §16.8
  budget). Above this, the formation is no longer real-time at the
  documented agent count.
- **cadence_throughput/*** — ticks/sec must scale roughly linearly with
  subscriber count (the broadcast fan-out is O(N) per tick).
- **consensus_latency/resolve/1000** — sub-100 ms. The tally loop is
  the only O(N) work in `ConsensusVote::resolve`; bigger numbers mean
  the per-ballot allocation got expensive.
- **environment_rcu/writes/1000** — sub-100 ms aggregate across the
  worker pool. ArcSwap contention shows up as a quadratic curve here;
  flat/linear means the RCU is healthy.
- **rally_cascade/detect_cascade/1000** — ≤ 33 ms. Above this, rally
  signals don't propagate within the cadence window and the formation
  feels "laggy" under load.

## Baseline numbers

> **Status:** Initial CI run pending — these numbers will populate
> once the bench suite has run in a stable environment. Local runs on
> developer hardware are not authoritative because criterion reports
> are sensitive to thermal throttling, background load, and CPU
> generation.

The intent is to capture the first stable-environment run's medians
here as a "do not regress" baseline. Regression-detection thresholds
recommended by criterion are ±3% on the median and ±5% on the p99 —
crossing either triggers a CI failure in the eventual bench-tracking
job.

## When to update this doc

- A new bench file lands in `crates/springtale-cooperation/benches/`
  and is wired into `Cargo.toml`.
- The §16.8 budget changes (would require a `COOPERATION_IMPLEMENTATION_PLAN.md`
  amendment first).
- A regression is captured + accepted as a new normal (rare; should be
  surfaced via PR review).

## See also

- `crates/springtale-cooperation/benches/*.rs` — bench sources, each
  with its own per-file doc comment explaining the measurement target.
- `docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md §10.6 / §16.8`
  — the contract these benches enforce.
