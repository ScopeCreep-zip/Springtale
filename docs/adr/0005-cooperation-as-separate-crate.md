# ADR 0005: Extract cooperation framework to its own crate

**Status:** Accepted
**Date:** 2026-04-10

## Context

Cooperation primitives — cadence, momentum, formations, blackboard,
stigmergy, contract net, mental model, rally, recovery, supervision,
and the 25-odd other modules — initially lived inside `springtale-bot`.
That made sense when the cooperation surface was small. As it grew
past 40 modules, three problems emerged:

1. **Compile times.** Any change to a cooperation primitive forced
   `springtale-bot` (and everything depending on it) to rebuild. The
   bot crate is the largest in the workspace; this was painful.
2. **Testing coupling.** Cooperation logic was testable only through
   bot-level integration tests. Pure-logic tests had to pull in the
   whole bot runtime.
3. **External consumers.** We want Python (`springtale-py`) and WIT
   (`springtale-wit`) bindings for cooperation **types and algorithms**
   without dragging the runtime, transport, sentinel, and connector
   dispatch along.

## Decision

Extract cooperation into `crates/springtale-cooperation/` as a
standalone crate with **zero internal Springtale dependencies**. Live
`Formation` structs (with mutable runtime fields like `active_task`,
`fuel`, `liveness`) stay in `springtale-bot::cooperation`. The
cooperation crate holds:

- Types (`FormationId`, `AgentHealth`, `IntentPattern`, …)
- Traits (`DynamicRole`, `Awareness`, etc.)
- Algorithms (interference detection, rally cascade, momentum
  transitions, mental-model learning)
- Per-agent loop steps (`sense`, `scan`, `react`, `respond_cfp`, `inbox`)
- Stigmergy surfaces, contract net protocol, consensus voting

`springtale-bot::runtime::event_loop::handle_cadence_tick` is the
14-step tick that calls into the cooperation crate per step. The bot
crate owns runtime state; the cooperation crate owns pure logic.

## Consequences

Positive:

- Cooperation has its own benchmarks (`crates/springtale-cooperation/benches/`)
  and fuzz targets (`crates/springtale-cooperation/fuzz/`) without
  pulling in the bot runtime.
- Python bindings work — `springtale-py` depends on
  `springtale-cooperation` and exposes a curated subset.
- WIT world (`springtale-wit`) targets cooperation types, not the
  bot runtime. WASM hosts can embed the *model* without the live
  daemon.
- Incremental compile times improved roughly 4x for cooperation-only
  changes.
- Easier to reason about. The crate boundary is the boundary between
  "what cooperation means" and "how the bot runs it".

Negative:

- More crate boundaries to keep in sync. Adding a new cooperation
  module touches `cooperation/src/lib.rs`, optionally
  `bot/src/cooperation/`, and the docs.
- Some types are duplicated — the cooperation crate has `Formation`
  (immutable shape) and the bot crate has its own live `Formation`
  with runtime fields. Two types, one name, controlled by module path.
  Confusing on first read.
- The split forced an explicit channel between cooperation events
  and the bot event loop (the `FormationCommand` mpsc + cooperation
  SSE stream). Worth it, but it's machinery.

Locks in:

- The cooperation crate's public API is now the API the cross-language
  bindings depend on. Breaking changes ripple to Python and WIT
  consumers.
- The 14-step tick is the canonical execution path. Adding a new
  per-tick concern means adding a step module under
  `bot/src/runtime/tick_steps/` and possibly a new cooperation
  primitive — not just inlining code somewhere.

## Alternatives considered

### Option A — Extract to its own crate, zero internal deps (picked)

Pros and cons enumerated above.

### Option B — Keep inside `springtale-bot`

Pros: simpler. One fewer crate boundary.
Cons: all the problems we set out to solve.

Why we didn't pick it: the compile-time and external-bindings issues
were urgent. The split paid for itself within weeks.

### Option C — Extract but allow it to depend on `springtale-core`

Pros: cleaner re-use of `Result`, error types, etc.
Cons: now Python bindings drag in `springtale-core`, which drags in
the rule engine, pipeline, canvas types. The "curated facade" for
Python becomes "the entire foundation".

Why we didn't pick it: zero-deps is load-bearing for the cross-
language story. The cost is duplicating a few error-handling
patterns; that's worth it.

### Option D — Split into multiple cooperation crates (one per concern)

Pros: even finer-grained compile boundaries.
Cons: 40+ small crates. Each one a `Cargo.toml`, a `lib.rs`, a set of
imports to keep in sync. The cognitive overhead would dwarf the
compile-time win.

Why we didn't pick it: too much process for too little payoff.

## References

- `crates/springtale-cooperation/` — the crate itself
- `crates/springtale-bot/src/cooperation/` — runtime glue
- `crates/springtale-bot/src/runtime/event_loop.rs` — 14-step tick
- `docs/guide/cooperation.md` — user-facing tour
- `docs/intended-arch/COOPERATION.md` — the design spec
- `docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md` — the
  10-week plan that drove the extraction
- Related: ADR 0006 (declarative schema — same simplification
  motivation)
