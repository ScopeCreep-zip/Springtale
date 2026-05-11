# Intervention

The intervention layer (L6) is the orchestrator's last-resort lever:
when a formation's cooperation primitives can't recover on their own,
intervention fires a specific verb against the formation to either
unstick it or escalate to you.

This page is the user-facing reference for what each intervention
*does*, when it fires, and how to read the `INTERVENTION` toasts that
land on the colony canvas.

The spec lives in
[`docs/intended-arch/COOPERATION.md §3.4`](../intended-arch/COOPERATION.md).

## When intervention fires

Once per cooperation tick the `check_interventions` step builds an
`InterventionSignals` snapshot and feeds it to the rule-based
evaluator (`crates/springtale-bot/src/orchestrator/intervention/`):

| Signal | What it counts |
| ------ | -------------- |
| `cascade_hits` | Consecutive ticks where rally cascade detection fired. |
| `rally_tokens` | Remaining self-rally tokens on the formation. |
| `cbba_stalled` | True when CBBA replan converged "stalled" instead of "converged". |
| `incapacitated_agents` | Members at health `Incapacitated`. |
| `operational_count` | Members still able to claim work. |
| `cold_duration_ticks` | Consecutive ticks the formation has been Cold. |
| `escalation_reason` | Supervisor's `Escalate { reason }` signal from B10. |

The evaluator runs **after** rally + supervision + replan. By the time
intervention sees a signal, the cheaper recovery layers have already
failed.

## The four interventions

### `ChangeIntent`

The formation switches to a different `IntentPattern` (e.g. from
`Execute` to `Stabilize`). Used when the current intent is provably
unreachable — for example, the orchestrator picks `Stabilize` after a
prolonged Cold run because progress isn't happening.

You see it as: `INTERVENTION CHANGE_INTENT — <summary>` on the
EventRibbon for ~4 seconds, plus the formation's intent badge in the
canvas updating.

### `InjectFuel { amount }`

The formation gets a one-shot fuel top-up so it can make at least one
more decisive action. Used when fuel exhaustion is the only thing
blocking recovery (the rally tokens are gone but the agents would
succeed if they had budget). Not free — fuel comes out of a global
intervention pool that fills at the daemon level, not per-formation.

You see it as: `INTERVENTION INJECT_FUEL — <amount>` plus the
formation's fuel bar jumping up.

### `ForcedDissolve { reason }`

The formation is wound down. The dissolve path runs normally:
synchronous member detach, mental-model persistence, knowledge-store
record_outcome (G2), gossip-bus outcome publish (G6). The only
difference from a user-initiated dissolve is that the reason field
records *which* intervention rule fired.

You see it as: `INTERVENTION FORCED_DISSOLVE — <reason>` and the
formation disappearing from the canvas. The `FormationOutcome` it
emits is sticky on the cross-formation gossip bus, so peer formations
that come online later still see what happened.

### `EscalateToUser`

The orchestrator has nothing left to try. The formation is left
running but flagged for user attention. A `SupervisorEscalated` event
fires on the cooperation event bus — the EventRibbon surfaces it as a
high-severity red toast, and the BottomPanel formation log records
the reason.

You see it as: `INTERVENTION ESCALATE_TO_USER — <reason>` plus a
persistent red badge on the formation card until you acknowledge by
acting on it.

## Reading the toasts

The EventRibbon filters to four high-severity event kinds:
`intervention_fired`, `supervisor_escalated`, `member_marked_down`,
`sacrifice_yield`. Each renders for 4 seconds with severity-coloured
borders:

- **Red border (`status-error`):** `intervention_fired`, `supervisor_escalated`
- **Yellow border (`status-warn`):** `member_marked_down`
- **Green border (`status-ok`):** `sacrifice_yield` (positive event)

If a toast disappears before you read it, the same envelope sits in
the BottomPanel formation log for the same formation, alongside every
other cooperation event that formation has produced.

## What to do when intervention fires

1. **`ChangeIntent`** — usually self-explanatory; the formation tells
   you what the new intent is. Decide if you want to override it back
   (via formation intent cycle on the command grid).
2. **`InjectFuel`** — informational; you don't usually act, but if you
   see fuel injections every minute the formation is running too hot.
   Consider raising the `fuel_budget` in `FormationConstraints`.
3. **`ForcedDissolve`** — read the reason, then decide whether to
   spawn a replacement formation with adjusted intent or members.
4. **`EscalateToUser`** — the reason field is your map. Common
   reasons:
   - `"rally tokens exhausted; no recovery path"` — the formation
     burned through all 3 self-rally attempts. Add a member with
     overlapping capabilities or reduce intent ambition.
   - `"cbba replan stalled past <N> ticks"` — task allocation
     couldn't reach equilibrium. Usually means a capability is
     missing across the entire formation; add a member with it.
   - `"cold duration <N> ticks exceeded budget"` — formation has
     been Cold long enough that the supervisor doesn't expect
     recovery. Reduce intent scope or change connectors.

## See also

- [troubleshooting-cooperation.md](troubleshooting-cooperation.md) for
  the full list of error codes and remediation steps.
- [`docs/intended-arch/COOPERATION.md §3.4`](../intended-arch/COOPERATION.md)
  for the formal intervention specification.
- `crates/springtale-bot/src/orchestrator/intervention/evaluator/rules.rs`
  for the exact thresholds each intervention rule uses.
