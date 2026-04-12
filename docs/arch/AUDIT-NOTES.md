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

## 3. Cooperation modules: type-defined but not wired ⚠

**Where:** `crates/springtale-bot/src/cooperation/`, consumed by
`runtime/event_loop.rs`.

The 20-file cooperation tree matches `docs/intended-arch/COOPERATION.md`
closely. The momentum machinery, cadence bus, formation struct,
environment blackboard, and orchestrator AI gating are **wired into the
hot path**. Several modules ship as type definitions only — they exist
and compile, but no site in the event loop invokes them yet.

```
  event_loop.rs  ───────────────────────────────────────────┐
         │                                                   │
         │  cadence_rx (broadcast)                            │
         ▼                                                    │
   cadence.rs   ◄─── WIRED                                    │
         │                                                    │
         ▼                                                    │
   formation.rs ◄─── WIRED      for every active formation    │
         │                                                    │
         ▼                                                    │
   momentum.rs  ◄─── WIRED      record_success, try_promote,  │
         │                      persist tier                  │
         ▼                                                    │
   environment.rs ◄─ WIRED      orchestrator posts subtasks   │
         │                      members pull them             │
         ▼                                                    │
   action.rs (SubTask) ◄─ WIRED (via orchestrator)            │
                                                              │
   ╔══════════════════════════════════════════════════════╗   │
   ║  TYPE-ONLY — types defined, never invoked from event ║   │
   ║  loop or any non-test code path                      ║   │
   ║                                                      ║   │
   ║  awareness    mental_model    attention              ║   │
   ║  consensus    commit          interference           ║   │
   ║  transformation  capability   rally                  ║   │
   ║  recovery     sacrifice                              ║   │
   ╚══════════════════════════════════════════════════════╝   │
                                                              │
   ╔══════════════════════════════════════════════════════╗   │
   ║  UNAUDITED — declared modules, not examined           ║   │
   ║                                                       ║   │
   ║  comms    handoff    pacing                           ║   │
   ╚═══════════════════════════════════════════════════════╝  │
                                                              │
                                                              ▼
                                                         bot event loop
```

*Fig. 1. Cooperation wiring. Only the first five modules are invoked from any non-test code path; the rest define the type system for future behavioural integration.*

| Module | Status | Missing integration |
|---|---|---|
| `cadence.rs` | ✓ wired | — |
| `formation.rs` | ✓ wired | — |
| `momentum.rs` | ✓ wired (promote/demote, persistence, capability gates) | — |
| `environment.rs` | ✓ wired (orchestrator posts subtasks; members pull) | — |
| `mental_model.rs` | ⚠ type only | Not persisted; not updated on tick |
| `awareness.rs` | ⚠ type only | `NeighborSnapshot` never populated |
| `rally.rs` | ⚠ type only | No dispatch on formation failure |
| `sacrifice.rs` | ⚠ type only | No voluntary-sacrifice decision point |
| `recovery.rs` | ⚠ type only | Distress signals never raised |
| `consensus.rs` | ⚠ type only | Vote machinery not invoked |
| `commit.rs` | ⚠ stub | `CommitPhase` enum present; no `tokio::sync::Barrier` |
| `interference.rs` | ⚠ type only | Detection not called in event loop |
| `transformation.rs` | ⚠ type only | Role transformation never triggered |
| `capability.rs` (DynamicCapabilitySet) | ⚠ type only | Not applied to members |
| `handoff.rs` | ? not audited | Spec says crossbeam-deque + sled |
| `pacing.rs` | ? not audited | Spec says governor GCRA |
| `comms.rs` | ? not audited | Spec says channel matrix |

**Impact:** Formations today exercise cadence → momentum → orchestrator
→ blackboard. Everything else in the cooperation spec is data-ready but
not behavioural. Failure recovery in particular falls back to generic
error handling rather than the rally / mutual-aid path in the spec.

**Fix path:** The cleanest first wire-up is distress signals — once
`recovery::DistressSignal` raises from a failed member, rally tokens
consume, role transformation becomes legible, and sacrifice gets a
decision site.

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

## 7. OpenAI streaming stub ⚠

**Where:** `crates/springtale-ai/src/adapter/openai_compat.rs:142-145`

**State:** `OpenAiCompatAdapter::stream()` returns
`AiError::NotImplemented("OpenAI streaming not yet implemented")`.
`complete()`, `parse_rule()`, `is_available()` all work. Anthropic and
Ollama both fully stream.

**Impact:** Users who configure an OpenAI-compatible endpoint (OpenAI,
Gemini, DeepSeek, llama.cpp) get non-streaming completions — their
fallback-parser experience has no incremental token output. Works
correctly, just feels laggy on long responses.

**Fix path:** SSE parsing pattern is the same as
`AnthropicAdapter::stream()` (`anthropic.rs:226-311`). The diff is the
event format.

---

## 8. Canvas broadcast drops old messages ⚠

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

## 9. Bot memory uncompressed ⚠

**Where:** `crates/springtale-store/src/migrations/002_bot.sql`

**State:** `bot_memory.content_encrypted` is a BLOB, AEAD-encrypted with
a per-row nonce, but not compressed. Long-running conversations with
large memory footprints will inflate the SQLite file more than necessary.

**Impact:** Disk usage only. No correctness concern.

**Fix path:** zstd-compress before encrypt; store `compression_algo` as
a new column for forward compatibility.

---

## 10. Audit trail retention is application-layer ⚠

**Where:** `crates/springtale-store/src/migrations/003_sentinel.sql`

**State:** `audit_trail` is append-only with three indices. No built-in
retention policy. Growth is unbounded.

**Impact:** Long-running instances accumulate audit rows indefinitely.
Acceptable for single-user daemons, needs attention if multi-user or
long-running deployments become common.

**Fix path:** Wire a retention task via `CronExecutor` calling
`StorageBackend::delete_audit_before(ts)`.

---

## 11. Formation blackboard log unbounded ⚠

**Where:** `crates/springtale-bot/src/cooperation/environment.rs`

**State:** `CooperativeBlackboard` keeps a `Mutex<Vec<BlackboardOp>>`
write log. No compaction, no bounded ring buffer.

**Impact:** Long-running formations grow memory steadily. Sibling of §10
but at the in-memory layer.

**Fix path:** Cap at N entries with drop-oldest semantics, or compact
after M operations.

---

## 12. Orchestrator AI call latency ⚠

**Where:** `crates/springtale-bot/src/orchestrator/orchestrate.rs`

**State:** At Fever tier, the orchestrator calls the AI adapter on
every cadence tick. No caching of decomposed subtasks; no deduplication
of identical intents.

**Impact:** A Stabilize intent held across 10 ticks makes 10 AI calls.
Cost and latency scale linearly with tick count.

**Fix path:** Content-hash the orchestration prompt; cache
`Vec<SubTask>` by prompt hash with a TTL. Invalidate on intent change.

---

## 13. No graceful shutdown for WASM epoch ticker ⚠

**Where:** `crates/springtale-runtime/src/init.rs:50-60`

**State:** The eternal tokio task that increments the Wasmtime epoch
every 1 s has no shutdown hook. On daemon shutdown it is force-killed
when the tokio runtime drops.

**Impact:** Zero in practice — it's just an atomic increment. Noted for
architectural hygiene only.

---

## 14. Rule creation is not transactional ⚠

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
2. **Cooperation hot-path wiring** (§3) — the `COOPERATION.md` type
   system is present but only half the behaviour is invoked from the
   event loop.
3. **Ergonomics** (§7 OpenAI stream, §12 orchestrator caching, §9
   compression) — non-blocking polish.

No critical, high, or security-relevant gaps. The cooperation framework
is the largest open area and the most visible divergence from intended
architecture.
