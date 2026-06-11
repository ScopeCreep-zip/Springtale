# Pacing

Pacing is Springtale's L4D-AI-Director-style throughput governor: the
formation cycles through five phases that each impose a different
GCRA rate limit on how fast agents can claim work. Different game
moments need different speeds — Peak is for the dramatic moment, not
for sustained ops.

The spec lives in
[`docs/intended-arch/COOPERATION.md §22`](../intended-arch/COOPERATION.md);
this page covers what each phase feels like and how to operate.

## The five phases

| Phase | L4D analog | What it does | Default quota |
| ----- | ---------- | ------------ | ------------- |
| `Preparation` | Build-up | Agents claim work slowly so they have headroom for the next phase. | 2 actions / min |
| `Active` | Sustained peak | The formation's normal working speed. | 10 actions / min |
| `Peak` | Crescendo | Brief high-throughput burst — used when a deadline is imminent. | 30 actions / min |
| `Recovery` | Peak fade | Throttle back hard so the formation cools down. | 1 action / min |
| `Disruption` | Director interrupt | All work pauses; the formation triages. | hard-block (every check rejected) |

Quotas are per-formation GCRA budgets (the `governor` crate), defined in
`crates/springtale-cooperation/src/pacing/quotas.rs`.

The cooperation tick's `check_pacing` step decides transitions; the
agent loop reads the current phase's quota off an `Arc<RateLimiter>`
via `ArcSwap`, so swaps are lock-free.

## How transitions fire

Phase transitions are driven by formation state, not wall clock:

- **Preparation → Active** when the formation reaches Hot tier with no
  active cascade.
- **Active → Peak** when the formation enters Fever tier *and*
  consensus has resolved a "go" vote on a high-priority objective.
- **Peak → Recovery** after 30 seconds in Peak or when the objective
  resolves, whichever comes first.
- **Recovery → Preparation** when momentum drops to Hot.
- **Anything → Disruption** when cascade detection trips or the
  supervisor escalates.
- **Disruption → Recovery** when the cascade clears.

Every transition emits a `PacingPhaseChanged { from, to }` event on
the cooperation event bus. The BottomPanel formation log shows them
as `PACING preparation → active`; the formation sprite on the canvas
applies a phase CSS class (`is-active`, `is-peak`, etc.) so you can
see the phase at a glance.

## Reading the canvas

A formation in each phase looks different on the colony canvas:

- **Preparation** — slight desaturation, neutral border.
- **Active** — full brightness, default border.
- **Peak** — yellow glow (`drop-shadow` with `--color-status-warn`).
  This is the only phase that flags visually that something
  high-priority is in flight.
- **Recovery** — desaturated + dimmed.
- **Disruption** — red glow + slow blink (the formation isn't
  *broken*, just paused).

If a formation sits in Recovery for too long it's a hint that
momentum decay is outpacing the formation's progress. Consider
adding members or reducing intent scope.

## When you want different quotas

The default per-phase quotas live in
`crates/springtale-cooperation/src/pacing/quotas.rs` as a static
table — deliberately one file, so tweaking (e.g. raising Peak from
30 → 60 actions/min) touches exactly one place. There is no runtime
knob for the phase quotas today; changing them means editing the table
and rebuilding.

For per-connector limits — a service whose remote API is stricter than
the phase quota — use the sentinel's per-connector rate limiter
(`[sentinel] rate_limit_per_minute`). It runs on every dispatch
regardless of formation phase, so the stricter of the two layers always
wins.

## When pacing fights you

Three failure modes recur:

### Symptom: agents idle even though work is on the blackboard.

The formation is in Preparation or Recovery; the rate limiter is
artificially throttling claims. Check `tracing` logs for `PacingPhaseChanged`
events near the time the work was posted. If you see a Recovery
phase, wait for it to clear; if you see a stuck Preparation, the
momentum tier is too low to transition out. Increase formation
member count or check for cascade signals.

### Symptom: Peak phase fires constantly.

The formation keeps hitting Fever then dropping back. Usually means
the formation's *natural* working tier is Fever (lots of throughput,
not much friction) but a single agent's degraded state keeps
triggering interference. Audit the formation's interference event
log for repeat offenders.

### Symptom: Disruption phase won't clear.

Cascade detection is firing every tick because the same set of
agents keeps failing. Read the latest cascade events in the
BottomPanel log; they'll point at which capability is missing. Add a
member with that capability or remove the offending agents.

## Pacing and consensus

Peak is the only phase where consensus matters for transition.
`Active → Peak` requires a vote — the formation literally votes
itself into the high-throughput window. This prevents a runaway
agent from forcing the formation into Peak unilaterally.

If your formation never reaches Peak even when you expect it to,
check that operational members are casting affirmative ballots on
the relevant vote (see [consensus.md](consensus.md)).

## See also

- [consensus.md](consensus.md) — the vote layer Peak transitions
  pass through.
- [`docs/intended-arch/COOPERATION.md §22`](../intended-arch/COOPERATION.md)
  — formal pacing spec, including the GCRA math.
- `crates/springtale-cooperation/src/pacing/` — implementation,
  including the per-tier quota table and the `ArcSwap`-driven
  rate-limiter swap.
