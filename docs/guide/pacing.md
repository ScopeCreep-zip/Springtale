# Pacing

Pacing is Springtale's port of the Left 4 Dead AI Director loop (Booth,
GDC 2009): intensity is *stress*, not work done. It rises when the
formation is harmed and decays over time — never while agents are
actively engaged. When it crosses the peak threshold the formation backs
off for a while, then builds up again. Only frequency changes; amplitude
(what an action does, how hard it is) never does.

## The four phases and the stress sample

Each processed tick folds one `StressSample` into intensity: `failures`
(reports whose action ran with alignment ≤ 0.5), `interferences`,
sentinel `throttles`, approval `denials`, all divided by member count;
`engaged` (any action taken) pauses decay. `BuildUp` runs at full tick
rate until intensity reaches 0.6; `SustainPeak` holds full rate for 4 s
(Booth: 3–5 s); `PeakFade` halves the tick rate until a natural break —
no action in flight or intensity below the threshold; `Relax` runs at a
quarter rate for 35 s (Booth: 30–45 s) and admits only read-only actions
(the formation senses but does not act), then returns to `BuildUp`.
`Disruption` (cascade / supervisor interrupt) resets to `BuildUp` on the
next tick. There are no per-phase action quotas; per-connector rate
limits belong to the sentinel. Constants live in
`crates/springtale-cooperation/src/pacing/manager.rs`.

## How transitions fire

Every transition emits a `PacingPhaseChanged { from, to }` event on
the cooperation event bus. The BottomPanel formation log shows them
as `PACING BuildUp → SustainPeak`; the formation sprite on the canvas
applies a phase CSS class (`is-active`, `is-peak`, etc.) so you can
see the phase at a glance.

## Reading the canvas

A formation in each phase looks different on the colony canvas:

- **BuildUp** — full brightness, default border.
- **SustainPeak** — yellow glow (`drop-shadow` with `--color-status-warn`):
  the formation is stressed and about to back off.
- **PeakFade** — slight desaturation, neutral border.
- **Relax** — desaturated + dimmed.
- **Disruption** — red glow + slow blink (the formation isn't
  *broken*, just paused).

A formation that keeps cycling through Relax is being harmed —
failures, interference, sentinel throttles or denials. Look at those
before touching the timings.

## When you want different timings

The peak threshold, sustain and relax periods, decay rate, and harm
weights are constants in
`crates/springtale-cooperation/src/pacing/manager.rs` — one file, so a
change touches exactly one place. There is no runtime knob today.

For per-connector limits — a service whose remote API is stricter than
the pacing phase — use the sentinel's per-connector rate limiter
(`[sentinel] rate_limit_per_minute`). It runs on every dispatch
regardless of formation phase, so the stricter of the two layers always
wins.

## When pacing fights you

Three failure modes recur:

### Symptom: agents idle even though work is on the blackboard.

The formation is in Relax, which admits only read-only actions. Check
`tracing` logs for `PacingPhaseChanged` events (they carry the
intensity) near the time the work was posted; Relax clears after 35 s.
If it keeps recurring, find what is stressing the formation.

### Symptom: SustainPeak fires constantly.

Intensity keeps crossing the threshold: the formation is repeatedly
harmed. Usually a single agent's degraded state keeps triggering
interference, or one connector keeps failing or being quarantined.
Audit the interference and sentinel event logs for repeat offenders.

### Symptom: Disruption phase won't clear.

Cascade detection is firing every tick because the same set of
agents keeps failing. Read the latest cascade events in the
BottomPanel log; they'll point at which capability is missing. Add a
member with that capability or remove the offending agents.

## Pacing and consensus

Pacing never touches what an action does or whether it needs a vote —
amplitude is the sentinel's and the consensus layer's business. In
`Relax` a mutating task is not claimed at all, so no proposal is
opened for it until the formation resumes `BuildUp`; read-only tasks
run throughout.

## See also

- [consensus.md](consensus.md) — the vote layer destructive actions
  pass through.
- [`docs/intended-arch/COOPERATION.md §22`](../intended-arch/COOPERATION.md)
  — the original pacing spec. Read it with the erratum at the head of §22:
  its "intensity = work done" reading is inverted relative to Booth and
  relative to the code.
- `crates/springtale-cooperation/src/pacing/` — implementation.
