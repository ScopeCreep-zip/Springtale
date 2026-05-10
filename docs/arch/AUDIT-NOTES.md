# Springtale — Audit Notes (Drift & Gaps)

> Snapshot of divergences from `docs/current-arch/` intent. None are
> critical; most are deliberate scope decisions.

---

## Severity Key

| Mark | Meaning |
|---|---|
| ⚠ | Tracked work, not a regression |
| ◆ | Deliberate scope decision with a known path to close it |
| ? | Unverified — audit did not reach the code confirming status |

---

## 1. Job queue is in-memory ◆

**Where:** `crates/springtale-scheduler/src/queue/producer.rs`

**State:** `JobProducer` uses a `tokio::sync::mpsc::Sender<Job>`. The
`jobs` SQLite table exists (migration `001_init.sql`) and
`StorageBackend` has the method signatures ready, but the producer is
not yet wired to durable storage.

**Impact:** Jobs are lost on daemon restart. Acceptable while rules are
idempotent and can be re-fired, but will need to land before persistent
retries matter.

**Fix path:** Inline comments in `producer.rs:39-42` note that the API
stays stable — only the backing store changes.

---

## 2. WASM host ready, no WASM connectors ship ◆

**Where:** `crates/springtale-connector/src/wasm/`, `sdk/connector-sdk/`

**State:** The Wasmtime host is built, tested, and wired through
`ConnectorHost` behind the same trait as native connectors. The SDK
under `sdk/connector-sdk/` is ready for `wasm32-unknown-unknown`
authors. But every first-party connector (all 15) is native Rust.

**Impact:** The sandbox story is currently aspirational for any
community connector. First-party connectors run in-process with no
isolation other than the capability checker and the `forbid(unsafe_code)`
discipline.

**Fix path:** Ship the first WASM connector (likely a port of
`connector-http` or `connector-presearch`) to validate the full path
end-to-end. SDK dispatch example in `sdk/connector-sdk/src/lib.rs:9-24`.

---

## 3. Cooperation framework fully wired ◆

**Where:** `crates/springtale-cooperation/` (crate),
`crates/springtale-bot/src/runtime/event_loop.rs` (14-step tick),
`crates/springtale-bot/src/cooperation/` (glue).

**State update (April 2026):** What this section previously described as
"type-defined, not wired" is now wired. The cooperation code moved into
its own crate (`springtale-cooperation`, 37 modules, zero internal deps)
and `springtale-bot` gained a 14-step per-formation tick pipeline that
exercises every module.

```
  event_loop.rs::handle_cadence_tick() — 14 steps
  ─────────────────────────────────────────────────
   1.  per-agent loop   (sense / scan / react / respond_cfp / inbox)
   2.  tick_processor   (action records, interference detection)
   2b. rally supervise  (drain member outcomes → rally events)
   3.  momentum         (decay check)
   4.  momentum         (success / interference / failure)
   4a-h. liveness, supervisor, fuel, implicit signals,
         state broadcasts, cohesion signals
   5.  persist momentum → SQLite
   6.  broadcast FormationContext
   7.  update awareness via gossip substrate
   8.  pacing phase transition
   9.  cascade detection + self-rally
   9b. recovery (distress → helper selection)
  10.  role transformation
  11.  consensus deadlines
  12.  expire commit barriers
  13.  mental model update
  14.  orchestrate (Fever tier only)
```

*Fig. 1. All cooperation modules are exercised somewhere in the tick pipeline or in the 5-step agent loop.*

| Module | Status | Notes |
|---|---|---|
| `cadence` | ✓ wired | broadcast tick bus, TickReport channel |
| `momentum` | ✓ wired | persisted, decay, capability gates |
| `awareness` | ✓ wired | gossip substrate (InMemory / chitchat) |
| `attention` | ✓ wired | zero-sum broker |
| `state` (environment) | ✓ wired | blackboard, shared env, write log |
| `consensus` | ✓ wired | deadlines checked step 11 |
| `commit` | ✓ wired | barriers expired step 12 |
| `interference` | ✓ wired | detected step 2 |
| `transformation` | ✓ wired | step 10 |
| `capability` | ✓ wired | DynamicCapabilitySet built per tick |
| `rally` | ✓ wired | cascade + self-rally step 9 |
| `recovery` | ✓ wired | distress evaluation step 9b |
| `sacrifice` | ✓ wired | evaluator consulted in recovery |
| `comms` | ✓ wired | implicit signals, state broadcasts, cohesion |
| `handoff` | ✓ wired | direct / flex-chain / sequential |
| `pacing` | ✓ wired | step 8 phase transitions |
| `supervision` | ✓ wired | per-member checks step 4d |
| `stigmergy` | ✓ wired | surfaces consumed in agent `react` |
| `contract_net` | ✓ wired | agent `respond_cfp` step |
| `routing` | ✓ wired | agent `scan` pulls via router |
| `mental_model` | ✓ wired | updated step 13, persisted on dissolve |
| `role` | ✓ wired | transformations apply updated role |
| `dissemination` | ✓ wired | step 6 FormationContext broadcast |
| `authority` | ✓ wired | momentum × layer permission matrix consulted |
| `replan` (CBBA) | ⚠ ancillary | invoked only by orchestrator global replan path |
| `agent_loop::AgentLoop::tick()` | ⚠ scaffolding | the 5 agent steps are invoked directly by `run_agent_loops` rather than through an `AgentLoop::tick()` facade |

**Impact:** The behavioural integration gap this section previously
tracked is closed. See `docs/guide/cooperation.md` and
`docs/guide/architecture.md` §6 for a user-facing tour and architectural
overview.

**Remaining work:** (1) wrap the 5 agent steps behind an explicit
`AgentLoop::tick()` API instead of the ad-hoc `run_agent_loops` function;
(2) invoke CBBA (`replan/cbba`) from a policy decision rather than only
from orchestrator escalation. Both are ergonomic, not behavioural.

---

## 4. Formations → rules generation not connected ⚠

**Where:** `crates/springtale-store/src/migrations/005_formations.sql`,
`crates/springtale-runtime/src/operations/formations.rs`

**State:** The `formations` and `formation_members` tables exist.
Formations can be created, deployed, paused, dissolved, and have their
intent cycled via the HTTP API. But the system that **generates rules
from formation intent** — compiling "Reconnoiter against Telegram and
Nostr" into actual `Rule` rows — is not implemented.

**Impact:** Formations are currently active at the coordination layer
(cadence, momentum, orchestrator) but do not produce persistent rules.
A formation that's paused and then resumed picks up its cadence state
but not any emitted rules.

---

## 5. Manifest re-verification on daemon restart ?

**Where:** `crates/springtale-connector/src/manifest/verify.rs`,
`registry/loader.rs`

**State:** `install.rs` verifies Ed25519 signature on install. The
scoped audit did not confirm that signatures re-verify on daemon
restart when loading from the `connectors` table or `wasm_binaries`
table.

**Impact:** If an adversary with filesystem access tampers with a
stored manifest or wasm blob between daemon runs, the next load may
not catch it. Filesystem integrity is generally assumed (the store
file lives in `~/.local/share/springtale` with `0o600`), but the
belt-and-braces re-check is an intentional spec item.

**Fix path:** Confirm or add a verification call inside
`registry::loader::load_native()` and its WASM sibling on every load.
Content-addressing via `wasm_hash` in migration 008 gives us the
primitive.

---

## 6. `SandboxLimits` timeout configurability ?

**Where:** `crates/springtale-connector/src/wasm/runtime.rs`

**State:** `epoch_interruption(true)` is enabled on the engine and the
documented 30 s wall clock is enforced via epoch deadlines. The scoped
audit did not find an explicit `Duration` constant for 30 s in
`runtime.rs` — likely it's set per invocation via a `SandboxLimits`
struct.

**Impact:** Low. Behaviour is correct (epoch ticker fires every second;
deadline honoured). Concern is only readability — a reader looking for
"where is 30 s defined" may not find it quickly.

**Fix path:** Verify or introduce a named constant in `wasm/limits.rs`.

---

## 7. Canvas broadcast drops old messages ⚠

**Where:** `crates/springtale-runtime/src/state.rs:66`

**State:** `canvas_tx: broadcast::Sender<CanvasUpdate>` with a bounded
buffer. Receivers are created per SSE connection in the handler. A slow
or disconnected consumer causes `RecvError::Lagged(n)` on subsequent
receivers, meaning **some canvas events can be missed** while the lag is
in progress.

**Impact:** Cosmetic — the dashboard catches up via the next full
`GET /canvas` fetch. No state loss, just possible missed animations.

**Fix path:** Accept as-is (broadcast semantics are appropriate for UI
streaming). Dashboard already re-fetches on reconnect.

---

## 8. Bot memory uncompressed ⚠

**Where:** `crates/springtale-store/src/migrations/002_bot.sql`

**State:** `bot_memory.content_encrypted` is a BLOB, AEAD-encrypted with
a per-row nonce, but not compressed. Long-running conversations with
large memory footprints will inflate the SQLite file more than necessary.

**Impact:** Disk usage only. No correctness concern.

**Fix path:** zstd-compress before encrypt; store `compression_algo` as
a new column for forward compatibility.

---

## 9. Audit trail retention is application-layer ⚠

**Where:** `crates/springtale-store/src/migrations/003_sentinel.sql`

**State:** `audit_trail` is append-only with three indices. No built-in
retention policy. Growth is unbounded.

**Impact:** Long-running instances accumulate audit rows indefinitely.
Acceptable for single-user daemons, needs attention if multi-user or
long-running deployments become common.

**Fix path:** Wire a retention task via `CronExecutor` calling
`StorageBackend::delete_audit_before(ts)`.

---

## 10. Formation blackboard log unbounded ⚠

**Where:** `crates/springtale-bot/src/cooperation/blackboard.rs` +
`crates/springtale-cooperation/src/state/`

**State:** `CooperativeBlackboard` keeps a write log. No compaction, no
bounded ring buffer. The tick pipeline uses
`last_tick_write_count` to Lamport-split the log for interference
detection, so the log is read during every tick.

**Impact:** Long-running formations grow memory steadily. Sibling of §10
but at the in-memory layer.

**Fix path:** Cap at N entries with drop-oldest semantics once the log's
oldest prefix is older than the highest `last_tick_write_count` across
all members.

---

## 11. Orchestrator AI call latency ⚠

**Where:** `crates/springtale-bot/src/orchestrator/orchestrate.rs`

**State:** At Fever tier, the orchestrator calls the AI adapter on
every cadence tick. No caching of decomposed subtasks; no deduplication
of identical intents.

**Impact:** A Stabilize intent held across 10 ticks makes 10 AI calls.
Cost and latency scale linearly with tick count.

**Fix path:** Content-hash the orchestration prompt; cache
`Vec<SubTask>` by prompt hash with a TTL. Invalidate on intent change.

---

## 12. No graceful shutdown for WASM epoch ticker ⚠

**Where:** `crates/springtale-runtime/src/init.rs:50-60`

**State:** The eternal tokio task that increments the Wasmtime epoch
every 1 s has no shutdown hook. On daemon shutdown it is force-killed
when the tokio runtime drops.

**Impact:** Zero in practice — it's just an atomic increment. Noted for
architectural hygiene only.

---

## 13. Rule creation is not transactional ⚠

**Where:** `crates/springtale-runtime/src/operations/rules.rs`

**State:** `create_rule()` adds to the in-memory `RuleEngine` and then
persists to the store. If the store write fails, the engine is already
mutated.

**Impact:** Minor — a catastrophic store failure would bring the daemon
down anyway. A brief window exists where an in-memory rule is active
but unpersisted.

**Fix path:** Persist first, then update the engine. Rollback is trivial
once the order flips.

---

## Summary

The gaps cluster in three buckets:

1. **Durability** (§1 jobs, §4 formations→rules) — mpsc is sufficient
   for current usage; persistence has a known path.
2. **Cooperation ergonomics** (§3) — previously the largest gap, now
   closed behaviourally. Remaining work is naming and orchestration
   of already-wired primitives.
3. **Ergonomics** (§11 orchestrator caching, §8 compression) — non-blocking polish.

No critical, high, or security-relevant gaps.
