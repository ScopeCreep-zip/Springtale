# Troubleshooting cooperation

Common failure modes when a formation isn't behaving the way you
expect, what to look for in the logs, and which `springtale fix
<id>` command to start with. Symptoms first; the corresponding error
IDs link to the per-error remediation guides.

## "The formation just sits there"

### Symptom: no agent claims work even though tasks are on the blackboard.

The most common cause is **pacing** — the formation is in Recovery
or Preparation phase and the rate limiter is throttling claims.

Check:

1. The BottomPanel formation log for recent `PACING` entries.
2. The momentum tier badge — if it's Cold, the formation never
   reaches Active.

Fix: see [pacing.md](pacing.md). If Cold, dissolve and spawn a
fresh formation with adjusted members; momentum recovery from Cold
is hard.

### Symptom: agents are idle but the formation is in Active phase.

The blackboard is genuinely empty, *or* every task has
`assigned_to` set and the assigned agent is degraded. The
non-degraded agents won't claim work that's been routed to a
specific peer.

Run `springtale fix COOP-A001` to see the no-capable-receiver
guide, but check liveness first:

```bash
springtale formation status <id>
```

If a member shows `Suspect` or `Down`, the supervisor will eventually
mark them down and re-route — give it a few ticks, or force-recover
via `springtale recovery trigger <agent_id>`.

## "The formation keeps cascading"

### Symptom: rally tokens drop to zero, intervention fires `ForcedDissolve`.

The cascade detector found a recurring failure pattern the formation
can't recover from. Read the dissolve reason on the EventRibbon
toast or the BottomPanel log.

Common reasons:
- `"rally tokens exhausted; no recovery path"` — too few members or
  wrong member capabilities. Add a member or swap a connector.
- `"cbba replan stalled past N ticks"` — task allocation can't find
  a stable assignment. Usually means a required capability is
  missing across the entire formation.
- `"cold duration N ticks exceeded budget"` — the formation has been
  Cold long enough that the supervisor doesn't expect recovery.

Fix: spawn a replacement with adjusted intent or members; the global
knowledge store will surface this dissolve's outcome to the new
formation so it doesn't repeat the mistake.

### Symptom: cascade fires every tick but the formation isn't dying.

Self-rally is working — each cascade triggers a recovery action
within the formation. You're seeing the work but not the recovery.

Check `springtale events list --formation <id>` for `RecoveryActionTaken`
events alongside the cascade hits. If recovery actions are
firing, the formation is healthy in the cascade-recovery sense — it
just lives in a turbulent regime. Either accept it or reduce intent
scope.

## "The orchestrator isn't deciding the way I want"

### Symptom: Fever-tier formation picks unexpected actions.

The orchestrator at Fever reads the mental model and the
cross-formation gossip bus before deciding. Either:

1. A high-confidence `domain_knowledge` entry is steering toward an
   action you don't want.
2. A peer formation's `FormationView` is making the orchestrator
   defer to that peer.

Inspect:

```bash
springtale memory audit --formation <id>
```

If a domain entry has `confidence: 0.9+` and is misleading, compact
the model:

```bash
springtale memory compact --max-entries 100
```

If a peer is misleading, dissolve the peer (it's probably in a bad
state).

### Symptom: orchestrator never decomposes the plan; the formation is stuck on `Reconnoiter`.

Reconnoiter is the formation's default intent on spawn. The
orchestrator only transitions to `Execute` once the mental model
has enough domain knowledge to plan with.

Two paths forward:
- Wait. Reconnoiter naturally accumulates domain entries.
- Seed manually: `springtale memory inject --formation <id>
  --description "user wants X" --confidence 0.7`.

## "Members keep getting marked down"

### Symptom: `MemberMarkedDown` toasts every few ticks.

The supervisor is liveness-checking each member every tick. A member
that misses too many beats gets marked `Suspect`, then `Down`.

Causes:
- **Network flap** — connector with intermittent connectivity. Check
  the connector's recent outputs for timeout errors.
- **Slow connector** — connector takes longer than the cadence tick
  to respond. The supervisor doesn't distinguish slow from dead.
  Reload the connector or increase its timeout.
- **Truly dead** — the member's underlying connector crashed. Run
  `springtale connector reload <name>` (G4 hot-reload) to bring it
  back.

### Symptom: a previously-down member won't recover even after the connector is back.

The supervisor uses an exponential backoff. After three down→up
cycles, the member is marked `Dead { recoverable: false }` and won't
auto-recover. Force a recovery:

```bash
springtale recovery trigger <agent_id>
```

If that fails too, the agent is truly stuck — dissolve and
re-spawn the formation.

## "I see strange consensus behaviour"

### Symptom: votes time out repeatedly.

Quorum failure. Either:

1. Every operational member already used their override token, so
   they default-vote "deny" on destructive actions.
2. Members are degraded and can't ballot.

Fix: dissolve and re-spawn the formation to reset override budgets
(see [consensus.md](consensus.md)), or upgrade the relevant agent's
autonomy:

```bash
springtale agent <id> autonomy up
```

### Symptom: votes always approve, even ones I'd expect to be controversial.

Either every agent is at Autonomous autonomy *or* the action's
manifest doesn't actually have `ApprovalPolicy::RequireConsensus`.
Check the connector's `manifest()` output. If the action is
classified `None`, no vote opens — that's a manifest-side decision.

## "Memory is misleading me"

### Symptom: new formations behave like prior formations even when I want a fresh start.

The global knowledge store (G2) seeds new formations with prior
outcomes. Wipe per-formation memory plus drop the global outcome
records:

```bash
springtale memory wipe --all --confirm
```

Or, more surgically, wipe just the offending formation's outcome
without touching the rest:

```bash
springtale memory wipe --formation <id>
```

(See [mental-model.md](mental-model.md) for the full memory model.)

## Error code reference

Every `CooperationError` variant has a stable `COOP-XXXX` ID and a
`springtale fix <id>` guide. The full table:

| Range | Module |
| ----- | ------ |
| 1001–1003 | cadence |
| 2001–2005 | formation |
| 3001–3002 | momentum |
| 4001–4002 | awareness |
| 5001–5003 | consensus |
| 6001–6005 | commit |
| 7001 | interference |
| 8001–8003 | rally |
| 9001–9003 | recovery |
| A001–A005 | handoff |
| B001 | pacing |
| C000–C005 | cross-cutting |

Look up any one with:

```bash
springtale fix COOP-8001
```

The guide prints common causes + suggested fixes + an auto-fix
attempt where one exists.

## Last resort

If you can't make sense of the formation's behaviour:

```bash
springtale trace --formation <id> --tail
```

Streams every cooperation event for that formation as JSON. Capture
30 seconds and file an issue with the trace — most failure modes
show up clearly in the event stream that aren't obvious from the
canvas.

## See also

- [intervention.md](intervention.md), [consensus.md](consensus.md),
  [sacrifice.md](sacrifice.md), [pacing.md](pacing.md),
  [mental-model.md](mental-model.md), [cross-formation.md](cross-formation.md)
  — each subsystem's own guide for deeper questions.
- `docs/guide/fixing-errors.md` for the original operational-error
  reference (which the `COOP-*` codes extend).
