# Cross-formation gossip

Two formations running in the same bot can see each other's
running-state through the cross-formation gossip bus (G6). It's how a
formation that just finished tells its peers what happened, without
those peers having to poll a database. Same primitive Quickwit uses
for cluster state; we use it for *formation* state.

The spec lives in
[`docs/intended-arch/COOPERATION.md §17.2`](../intended-arch/COOPERATION.md)
and `COOPERATION_IMPLEMENTATION_PLAN.md §12.2`.

## What's gossiped

The bus carries two kinds of payloads:

### `FormationView` (per-tick)

Every operational formation publishes one of these per cooperation
tick, via the `publish_formation_view` step in the tick loop:

```rust
FormationView {
  formation_id, intent, momentum_tier,
  operational_count, member_count, rally_tokens_remaining,
  status, at,
}
```

This is the *soft state* — small, high-churn, never durable. Peers
read it to make adaptive decisions ("formation A is at Fever; I'll
back off on rally tokens since they're the priority right now").

### `FormationOutcome` (on dissolve)

A single `FormationOutcome` fires when a formation dissolves:

```rust
FormationOutcome {
  formation_id, final_intent, success_count, failure_count,
  dissolve_reason, at,
}
```

This is *sticky*: subscribers that come online after the dissolve
still see the outcome via the bus's replay buffer. That's how a new
formation's spawn hook can read "what just finished" without
race-conditioning on the dissolve.

(The same data also lands in the durable global knowledge store —
see [mental-model.md](mental-model.md). The bus and the store are
intentionally separate: the bus is for live decisions, the store is
for forever.)

## The subscriber model

Each formation gets a filtered subscriber view: its own deltas are
*excluded* from its subscription. A formation never receives its own
broadcasts back. The filtering happens at the bus, not in the
subscriber, so the formation can subscribe naively without having to
remember its own id.

Subscriber API:

```rust
let mut rx = bus.subscribe(my_formation_id);
while let Some(delta) = rx.recv().await {
    match delta {
        FormationDelta::View(view) => /* react to peer running state */,
        FormationDelta::Outcome(out) => /* react to peer dissolve */,
    }
}
```

The in-memory implementation
(`crates/springtale-cooperation/src/gossip/bus.rs::InMemoryFormationGossipBus`)
is the default. A chitchat-backed implementation can land behind the
same trait when cross-process federation is needed; the existing
`ChitchatGossipStore` for per-agent awareness shares the same
substrate.

## When you'd actually use this

The bus has three live consumers in the bot today:

1. **Orchestrator at Fever tier.** When the orchestrator decomposes a
   plan, it reads the snapshot via `bus.snapshot()` to see what other
   formations are doing. If formation B just finished the same intent
   successfully, the orchestrator's prompt includes B's outcome as
   context.
2. **Cooperation event ribbon (frontend).** The colony canvas
   subscribes via the SSE stream `/cooperation/events` and renders
   peer-formation state changes as informational entries.
3. **Future: cross-formation rally.** When a formation's rally tokens
   are exhausted but a peer is in Preparation, the peer can volunteer
   slack capacity. The protocol design exists; the actual wiring is
   on the roadmap (it's not yet a tick step).

## What's *not* gossiped

- **Task contents.** The blackboard is per-formation; tasks don't
  leak.
- **Agent identities.** `FormationView` carries counts, not
  individual `AgentId`s.
- **Mental-model state.** The mental model is per-formation
  per-storage; cross-formation knowledge transfer uses the global
  knowledge store, not the bus.
- **Connector outputs.** Connector outputs route through the
  per-formation event bus and the audit log; peer formations don't
  see them.

This is intentional. The cross-formation bus is for *governance-level*
visibility, not data sharing.

## Common questions

**Why isn't formation B picking up the slack when formation A's
rally is exhausted?**

Cross-formation rally isn't wired yet. Peer formations *can see*
each other's rally state via the bus, but the orchestrator doesn't
yet act on it. Track the wiring in the project plan.

**Will the bus survive bot restart?**

`FormationView` deltas don't — they're live state. `FormationOutcome`
records are also dropped from the bus on restart, but the *same data*
is persisted to the global knowledge store, so spawn-time seeding
still works. The bus is intentionally ephemeral; durability lives
in the knowledge store.

**Two formations have the same id. Is that a problem?**

`FormationId` is a fresh UUID per spawn, so collisions are
astronomically unlikely. If you somehow see two formations with the
same id, that's a bug — file an issue.

**Can a connector publish to the bus?**

No. The bus is internal to the cooperation layer; connectors
publish events via the per-connector event channel
(`/events/stream`), not the cross-formation bus.

## See also

- [mental-model.md](mental-model.md) — the durable cross-formation
  store the bus complements.
- [pacing.md](pacing.md) — peers use bus-observed momentum to time
  their own phase transitions.
- `crates/springtale-cooperation/src/gossip/` for the trait,
  in-memory impl, and tests.
