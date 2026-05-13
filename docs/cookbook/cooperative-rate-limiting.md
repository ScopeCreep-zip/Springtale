# Recipe: Cooperative rate limiting

Two bots share an API rate limit (a single OpenAI key shared between
a moderation bot and a research swarm; a Discord bot token shared
between a moderator and an announce bot). You want them to coordinate
who gets to send next, not just race.

This recipe uses the **cooperation framework's pacing module**, which
implements GCRA (Generic Cell Rate Algorithm — the same algorithm
Linux's `tc` and many CDNs use). Pacing is per-formation; if both
bots are members of the same formation, they share its pacing state.

## Setup: one formation, two members

```toml
[rule]
name = "shared-pacing-bot"
enabled = true

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message_received"

# Both moderation and announce bots are members of the same formation.
# They share the pacing GCRA state and the blackboard.
[[formation]]
id = "comm-bots"
intent = "Stabilize"

[[formation.members]]
agent_id = "moderation-bot"

[[formation.members]]
agent_id = "announce-bot"

[[formation.pacing]]
# GCRA parameters.  See guide/pacing.md for the full model.
budget = 30            # 30 requests
window_secs = 60       # per 60 seconds
emission_interval_secs = 2.0   # min gap between requests
```

The `[[formation.pacing]]` block configures the GCRA limiter. With
budget=30, window=60, emission=2, the formation allows up to 30
calls per minute with at least 2 seconds between any two calls.

## Members call through the formation

A member's action goes through the formation's pacing check:

```toml
[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"
pacing_via = "comm-bots"        # check formation comm-bots' pacing

[actions.params]
channel_id = "..."
text = "${trigger.text}"
```

Behind the scenes:

1. Member's per-agent loop runs the `respond_cfp` step.
2. Before dispatch, the runtime asks the formation's pacing module
   `can_emit_now()`.
3. If yes, dispatch proceeds; pacing state is updated.
4. If no, the action waits (delayed dispatch) or is dropped (per
   `pacing_overflow_policy`).

The two bots competing for the same shared budget are arbitrated by
the formation's pacing state, not by racing on the network.

## Overflow policies

```toml
[[formation.pacing]]
budget = 30
window_secs = 60
emission_interval_secs = 2.0
overflow_policy = "delay"       # one of: delay, drop, escalate
max_delay_secs = 30
```

- **`delay`** — caller blocks up to `max_delay_secs` waiting for a
  slot. Default. Right for "I'd rather wait than skip".
- **`drop`** — caller returns immediately with a rate-limited error.
  Right for "skip if I can't send right now".
- **`escalate`** — caller goes through the orchestrator (if at Fever
  tier). Right for "decide dynamically whether to send".

## Pacing across formations

Pacing is per-formation. Two separate formations don't share state.
If you want **cross-formation** shared rate-limiting, the gossip
bus carries the necessary signal:

```toml
[[formation.pacing]]
gossip_aware = true     # publish FormationView with current GCRA state
```

Other formations can read each other's `FormationView` and back off
when peers are heavy. This is voluntary cooperation, not enforcement —
a misbehaving formation can ignore peers' state. The cooperation
framework explicitly doesn't try to enforce cross-formation behaviour;
see [`docs/intended-arch/COOPERATION_SECURITY_REVIEW.md`](../intended-arch/COOPERATION_SECURITY_REVIEW.md)
on this design choice.

## When NOT to use formation pacing

- **Single-bot, no peers** — just use the sentinel's per-connector
  rate limiter in `[sentinel] rate_limits`. Simpler, no formation
  needed.
- **Multi-process** — pacing state lives in the daemon process. Two
  daemons can't share pacing yet. (Phase 3 / Veilid will fix this.)
- **Hard real-time guarantees** — GCRA's "you can emit now" check is
  fast but tokio-async. If you need microsecond precision, this
  isn't it.

## Sentinel rate limiter vs formation pacing

Two layers, both useful:

| Layer | Scope | Purpose |
|---|---|---|
| `[sentinel] rate_limits` | Per-connector global | Safety net — protects the **service** from overload regardless of which rule is calling |
| `[[formation.pacing]]` | Per-formation | Coordination — lets multiple agents share a budget cooperatively |

Use both. The sentinel layer is the floor; formation pacing is the
ceiling per cooperating group.

## Inspecting pacing state

```bash
springtale-cli formation get comm-bots --include=pacing
```

Output includes current GCRA state: tokens remaining, last emission
time, queue depth (for `delay` policy).

The dashboard renders this in the formation detail card as a
horizontal bar showing budget utilization.

## Gotchas

- **GCRA isn't a token bucket.** Tokens regenerate continuously
  (decay-based), not in chunks at window boundaries. Burstier than
  a fixed-window limiter; smoother than a leaky bucket.
- **`emission_interval_secs`** is the minimum gap between any two
  emissions. If you set it to 0, you get token-bucket semantics
  (instantaneous bursts up to budget).
- **`gossip_aware` adds network overhead.** Each gossip-aware
  formation publishes a `FormationView` every tick. For two
  formations the cost is trivial; for 100, set `gossip_aware = false`
  unless you genuinely need cross-formation backoff.
- **Pacing doesn't replace retries.** Per-connector retry logic
  still handles transient failures. Pacing decides "should I emit
  now"; retries decide "what to do when the network ate my emit".
- **`overflow_policy = "delay"` with a slow `max_delay_secs` blocks
  agents.** During the delay the agent's `respond_cfp` step is
  parked. Other agents in the same formation continue, but the
  delayed one isn't doing other work.
