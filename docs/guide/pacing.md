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
| `Preparation` | Build-up | Agents claim work slowly so they have headroom for the next phase. | 1 req / 2s per agent |
| `Active` | Sustained peak | The formation's normal working speed. | 5 req / s per agent |
| `Peak` | Crescendo | Brief high-throughput burst — used when a deadline is imminent. | 20 req / s per agent, capped at 30 s wall time |
| `Recovery` | Peak fade | Throttle back hard so the formation cools down. | 1 req / 5s per agent |
| `Disruption` | Director interrupt | All work pauses; the formation triages. | 0 req / s (no work claimed) |

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

The default per-tier quotas live in
`crates/springtale-cooperation/src/pacing/quotas.rs` as a static
table. Practical reasons to override:

- **Connector with a stricter remote rate limit** — if Slack's
  webhook is `1 req/s`, the formation should never claim faster
  than that even at Peak. Per-connector overrides are declared in
  the connector's manifest and merged into the formation's effective
  quota at install time.
- **Formation that's IO-bound rather than CPU-bound** — for HTTP-only
  formations, Peak quota can be much higher (50 req/s) without
  contention. Configure via `FormationConstraints::pacing_overrides`.

There's no UI knob for pacing overrides; they're configured per
formation via the `springtale formation config` CLI or in code.

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
