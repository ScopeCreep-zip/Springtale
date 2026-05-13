# Performance reference

Numbers for what Springtale costs to run. Everything here is
measured against the workspace's reference benchmarks
(`crates/springtale-cooperation/benches/`) and a typical
laptop-class machine (Apple M-series / x86_64 with 16 GB RAM and
NVMe SSD). Your numbers will vary; the order of magnitude shouldn't.

For the deep dive on cooperation-specific perf,
[`docs/benchmarks/cooperation-scaling.md`](../benchmarks/cooperation-scaling.md)
has the actual cargo-criterion output.

## TL;DR

| Resource | Idle | Active (typical) | Stressed |
|---|---|---|---|
| RAM | 30 MB | 150 MB | 500 MB |
| CPU | <1% | 5% sustained | 50% bursty |
| Disk | 2 MB binary | + vault + db | + audit log |
| Cold start | 200ms | — | — |
| HTTP request latency | — | <5ms p99 (local) | — |
| Cooperation tick | — | <10ms p99 | <50ms p99 |
| Sentinel verdict | — | <500µs p99 | <2ms p99 |

"Active typical" means a handful of formations, 5 connectors,
a few hundred actions per hour. "Stressed" means tens of formations,
sustained webhook firehose, AI tool calls under load.

## Binary size

| Component | Size |
|---|---|
| `springtaled` (release, stripped) | ~12 MB |
| `springtale-cli` (release, stripped) | ~6 MB |
| Tauri desktop bundle (.dmg / .deb / .msi) | ~25 MB |

The static-musl build (`nix build .#springtaled-static`) is ~15 MB
— slightly larger because we statically link everything.

By comparison: an Electron-equivalent desktop app would be 150-250 MB,
and an Anthropic / OpenAI client built directly on those SDKs would
have the same crypto + TLS + HTTP cost as ours plus less safety.

## Memory

### Idle daemon

A fresh `springtaled` boot with no connectors, no rules, no
formations uses about 30 MB resident.

Breakdown (approximate, via `heaptrack`):

| Component | RAM |
|---|---|
| `tokio` runtime + executor | ~8 MB |
| `rustls` + root certs | ~3 MB |
| `wasmtime` engine + cranelift | ~10 MB |
| `rusqlite` + SQLite3MultipleCiphers | ~4 MB |
| Everything else | ~5 MB |

The bulk is wasmtime's engine. Compiling a fresh WASM connector
adds ~5 MB per module.

### Per-formation memory

A formation costs about 0.5–2 MB resident depending on:

- Blackboard size (grows linearly with writes; bounded by config).
- Mental model contents.
- Number of members.

For 100 active formations: ~150 MB. The cooperation-scaling
benchmarks show linear scaling up to ~1000 formations on a 16 GB
machine.

### Per-connector memory

Native connectors: <1 MB each — they're just Rust structs.

WASM connectors: each instance allocates a 64 MB sandbox (configurable
via `[connector.wasm] memory_limit_pages`). Actual usage is bounded
by what the WASM code allocates. A simple webhook connector might
use ~5 MB resident.

## CPU

### Idle

CPU usage at idle is dominated by tokio's reactor wakeups (~1
context switch per second from the heartbeat) and the wasmtime
epoch ticker (1Hz). Total: <1% on a single core.

### Per-tick cooperation cost

A 14-step formation tick takes 1–10 ms on a typical machine. The
expensive steps are:

| Step | Typical cost |
|---|---|
| 1 — per-agent loop | 0.5–2 ms per agent |
| 4 — momentum update | <100 µs |
| 5 — momentum persist (SQLite) | 1–3 ms |
| 13 — mental model update | 0.5–2 ms |
| 14 — orchestrate (AI) | 100ms–10s per call |

A formation at Cold/Warming tier (no AI calls) ticks in <5 ms total.
A Fever-tier formation calling the AI adapter per tick is dominated
by AI latency.

### Per-action dispatch

Sentinel verdict path: <500 µs p99. The four checks (circuit-breaker,
rate-limit, dead-man, impact-classify + approval) are all in-memory.

Connector dispatch is dominated by the connector's actual work
(network I/O, file I/O, AI call). Pre-dispatch overhead is <1ms.

### Wasmtime sandbox

Cold compile of a fresh WASM module: 50–200 ms (cranelift's job).
Subsequent invocations against the same module: <1 ms overhead from
the sandbox itself.

The G-series WASM tier cache (`crates/springtale-connector/src/wasm/tier/cache.rs`)
caches compiled modules so the cold compile happens once.

## Disk

### Binary

`/usr/local/bin/springtaled`: ~12 MB.

### Data directory

| File | Typical size | Growth driver |
|---|---|---|
| `vault.bin` | 131,152 bytes (constant) | — |
| `springtale.db` | 1–100 MB | event volume, audit retention |
| `springtale.db-wal` | <10 MB transient | write rate |
| `audit.log` | varies | rotation policy |
| `connectors/` | 5–50 MB | number of WASM connectors |
| `api_token` | <100 bytes | — |

### Database growth

Under typical load (5 connectors, 100 actions/hour, 90-day audit
retention):

- `audit_trail`: ~10 MB / month
- `events`: ~5 MB / month (with rolling purge)
- `bot_memory`: ~1 MB / month per active bot session
- `mental_model_*`: ~100 KB / month per active formation
- `formation_momentum`: ~50 KB / month per active formation

The DB grows roughly 50–100 MB / month under that profile. Heavy
moderation bots can hit 500 MB / month.

Configure retention to bound growth. See
[`docs/operations/database.md`](../operations/database.md).

## Network

### Idle

Zero outbound. We do not phone home. Verifiable via `tcpdump`.

### Per-action

Dispatching an action makes one outbound HTTPS call (to the
connector's target service). The TLS handshake is amortised via
`reqwest`'s connection pool — typically <100 ms for a fresh
endpoint, <10 ms for a reused connection.

### Webhook receipt

A webhook delivery is ~5 ms of CPU on the daemon side: HMAC verify,
dispatch decision, queue insertion. Doesn't block subsequent
requests.

## Cooperation scaling

Detailed in [`docs/benchmarks/cooperation-scaling.md`](../benchmarks/cooperation-scaling.md);
summary:

| Formations | RAM | CPU (idle) | Tick latency p99 |
|---|---|---|---|
| 10 | 80 MB | <1% | 5 ms |
| 100 | 150 MB | 2% | 10 ms |
| 500 | 400 MB | 8% | 25 ms |
| 1000 | 700 MB | 15% | 50 ms |
| 5000 | 3 GB | 40% | 200 ms |

Beyond 5000 formations, the cooperation tick begins to fall behind
the cadence bus (default 1Hz). For very large fleets, either lower
the cadence rate (`[cooperation] cadence_interval_secs = 5`) or
split across multiple daemons (Phase 3 / Veilid concern for cross-
daemon coordination).

## AI adapter latency

| Adapter | First-token | Total (100 tokens) |
|---|---|---|
| `OllamaAdapter` (local llama3.1:8b) | 200 ms | 5–15 s |
| `AnthropicAdapter` (Claude Sonnet 4.6) | 400 ms | 2–4 s |
| `OpenAiCompatAdapter` (gpt-4) | 500 ms | 3–5 s |
| `NoopAdapter` | <100 µs | <100 µs |

For tool-calling flows, multiply by `max_tool_iterations`.

## Startup

| Phase | Time |
|---|---|
| Process start → tokio runtime | 20 ms |
| Vault unlock (Argon2id KDF) | 50–200 ms |
| Database open + schema apply | 50–100 ms |
| Connector registry load | 20 ms per connector |
| HTTP server listening | 50 ms |
| **Total cold start** | **200–500 ms** |

Argon2id KDF is the dominant cost. We tuned its parameters (64 MB,
3 iterations, 4 parallelism) for ~200 ms on a typical machine —
fast enough to feel snappy, slow enough to make brute-force
prohibitive.

## Where to measure on your hardware

```bash
# Cold-start time:
time (springtale-cli init && springtale-cli server start --once)

# Memory under load:
ps -p $(pgrep springtaled) -o rss

# Per-tick latency:
springtale-cli trace --formation <id> | grep "tick_completed"

# Sentinel verdict latency:
RUST_LOG=springtale_sentinel=debug springtaled 2>&1 | grep "verdict_emitted"

# Cooperation benchmarks (your machine):
cargo bench -p springtale-cooperation
```

## Profiling

For deeper analysis:

```bash
# Flame graph of the daemon under load:
cargo flamegraph --bin springtaled

# Memory profiling:
heaptrack ./target/release/springtaled

# Async task tracing (tokio-console):
TOKIO_CONSOLE_BIND=127.0.0.1:6669 springtaled
tokio-console
```

The cooperation benches use `cargo-criterion` for statistical rigour.
Single-run timing can be misleading; run benches three times and
take the median.

## Where Springtale is NOT designed to be fast

Some things we explicitly don't optimise for:

- **Sub-millisecond formation ticks.** The cadence is 1 Hz by
  default; aggressive sub-tick logic isn't in the design.
- **Tens of thousands of concurrent webhook deliveries.** The
  daemon can absorb bursts up to ~1000/sec but isn't designed as a
  load-balancing edge — put a real load balancer in front if
  you need that.
- **Multi-process coordination.** Pacing, sentinel, audit are all
  per-daemon. Cross-daemon is a Phase 3 concern.
- **Cold-start under 100ms.** The Argon2id passphrase derivation
  deliberately costs 200ms. We're not going to give that up.

## When to worry

Tuning becomes urgent when:

- RAM exceeds 1 GB on a single daemon. Usually a runaway formation
  that won't dissolve.
- Tick latency exceeds 100 ms consistently. Either too many active
  formations or an orchestrator stuck in an AI loop.
- Database larger than 1 GB. Audit retention too generous; set a
  shorter `audit_retention_days`.
- Cold start over 1s. Vault file corruption; run `springtale-cli
  doctor`.
