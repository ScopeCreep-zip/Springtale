# Recipe: Cooperative rate limiting

Two bots share an API rate limit (a single Discord bot token shared
between a moderator and an announce bot; one upstream API key shared by
a research swarm). You want them to coordinate who gets to send next,
not just race.

This recipe uses the **cooperation framework's pacing module**, which
implements GCRA (Generic Cell Rate Algorithm — the same algorithm
Linux's `tc` and many CDNs use, via the `governor` crate). Pacing is
per-formation: if both bots are members of the same formation, they
share its pacing state automatically. There is nothing to configure —
pacing is phase-driven, not a TOML block.

## Setup: one formation, two members

Create a formation and add both agents as members — from the desktop
Team Builder, or over the API:

```bash
curl -X POST http://127.0.0.1:8080/formations \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "comm-bots", "intent": "Stabilize"}'

# Add members by their connector (id from the create response):
curl -X POST "http://127.0.0.1:8080/formations/$FID/members" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"connector_name": "connector-discord"}'
```

From this point the two members draw from one GCRA budget. There is no
`pacing_via` routing to declare — membership *is* the routing.

## How the budget is decided

The formation's current **pacing phase** picks the quota
(`crates/springtale-cooperation/src/pacing/quotas.rs`):

| Phase | Actions/min | When |
|---|---:|---|
| `Preparation` | 2 | build-up, information gathering |
| `Active` | 10 | normal work pace |
| `Peak` | 30 | brief maximum-throughput burst |
| `Recovery` | 1 | cooldown, consolidation only |
| `Disruption` | hard-block | sentinel-detected anomaly |

Phase transitions are evaluated every cooperation tick (step 8,
`check_pacing`) from formation state — momentum tier, cascade signals,
objective progress — not wall clock. See
[`guide/pacing.md`](../guide/pacing.md) for the full phase model and
transition rules.

Behind the scenes on every member dispatch:

1. The member's agent loop wants to emit an action.
2. The formation's `RateLimiter` (lock-free `ArcSwap`, shared by all
   members) answers "can you emit now?" against the current phase's
   GCRA budget.
3. If yes, dispatch proceeds and the GCRA state advances.
4. If no, the claim stays parked on the blackboard — another tick will
   retry. Work is deferred, not dropped.

The two bots competing for the same budget are arbitrated by shared
formation state, not by racing on the network.

## Pacing across formations

Pacing is per-formation. Two separate formations don't share state.
For **cross-formation** awareness, the gossip bus already carries
`FormationView` snapshots between sibling formations every cooperation
tick — a formation's view includes its pacing phase, so peers can back
off when a sibling is running hot.

This is voluntary cooperation, not enforcement — a misbehaving
formation can ignore peers' state. The cooperation framework
explicitly doesn't try to enforce cross-formation behaviour; see
[`docs/intended-arch/COOPERATION_SECURITY_REVIEW.md`](../intended-arch/COOPERATION_SECURITY_REVIEW.md)
on this design choice.

## When NOT to use formation pacing

- **Single-bot, no peers** — the sentinel's per-connector rate limiter
  (`[sentinel] rate_limit_per_minute`) already protects the upstream
  service. Simpler, no formation needed.
- **Multi-process** — pacing state lives in the daemon process. Two
  daemons can't share pacing yet. (Phase 3 / Veilid territory.)
- **Hard real-time guarantees** — GCRA's "can you emit now" check is
  fast but tokio-async. If you need microsecond precision, this
  isn't it.

## Sentinel rate limiter vs formation pacing

Two layers, both useful:

| Layer | Scope | Purpose |
|---|---|---|
| `[sentinel] rate_limit_per_minute` | Per-connector, global | Safety net — protects the **service** from overload regardless of which rule or agent is calling |
| Formation pacing | Per-formation, phase-driven | Coordination — lets multiple agents share a budget cooperatively |

Use both. The sentinel layer is the floor; formation pacing is the
ceiling per cooperating group. The stricter layer always wins.

## Inspecting pacing state

The formation detail card on the colony canvas shows the current
pacing phase (with a CSS class per phase — Peak gets the yellow glow),
and `PacingPhaseChanged` events arrive as `cooperation` frames on the
multiplexed SSE stream (one-time ticket, never a token in the URL):

```bash
TICKET=$(curl -s -X POST "http://127.0.0.1:8080/stream/ticket" \
  -H "Authorization: Bearer $TOKEN" | jq -r .ticket)
curl -N "http://127.0.0.1:8080/stream?ticket=$TICKET" | grep -A1 "^event: cooperation"
```

Filter on `"formation_id":"$FID"` in the frame data.

`tracing` logs carry the same transitions for headless installs.

## Gotchas

- **GCRA isn't a token bucket.** Cells regenerate continuously
  (decay-based), not in chunks at window boundaries. Burstier than a
  fixed-window limiter; smoother than a leaky bucket.
- **Quotas are compile-time today.** The per-phase table is a static
  map in `quotas.rs` — changing it means editing one file and
  rebuilding. The runtime-configurable layer is the sentinel limiter.
- **Recovery throttles hard (1/min).** If agents look idle with work
  on the blackboard, check the pacing phase before assuming a bug —
  a formation cooling down from Peak is *supposed* to crawl.
- **Disruption blocks everything.** A cascade or sentinel anomaly
  hard-blocks all emissions until it clears. That's the point.
- **Pacing doesn't replace retries.** Per-connector retry logic still
  handles transient failures. Pacing decides "should I emit now";
  retries decide "what to do when the network ate my emit".
