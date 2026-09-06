# Mental model

Springtale formations carry a `SharedMentalModel` (§21) that records
what they've learned during a mission. Some of that learning persists
between missions — when a new formation spawns, it seeds its model
from prior outcomes. This page explains what's preserved, what's not,
and how to operate against it.

The spec lives in
[`docs/intended-arch/COOPERATION.md §21`](../intended-arch/COOPERATION.md).

## Two layers

There are two distinct persistence layers, both backed by SQLite under
the vault directory (encrypted at rest):

### 1. Per-formation mental model

Each formation has its own `SharedMentalModel` containing:

- **`domain_knowledge`** — facts the formation has learned (e.g. "this
  user prefers JSON responses", "this connector rate-limits at 60 req/min").
- **`capability_awareness`** — what each member knows about *other*
  members' capabilities (Siege "Engineer has hard breach").
- **`cooperation_patterns`** — sequences that worked before (MH "when
  monster topples, hammer goes to head, cutter goes to tail").
- **`shared_vocabulary`** — terms the formation has established (room
  names, monster parts, internal jargon).
- **`conventions`** — emergent rules ("in this formation, agent A
  usually handles X while agent B handles Y").
- **`graph`** — a `petgraph` knowledge graph linking the above.

This model survives both a dissolve and a daemon restart, keyed by
`formation_id`. On `Dissolve` the command handler calls
`lifecycle::persist_mental_model` before the `Formation` is dropped
(`crates/springtale-bot/src/cooperation/lifecycle.rs`), and
`spawn_formation` reloads it when a formation is next deployed against the
same id — so a redeployed formation warm-starts with what its predecessors
learned, at Cold momentum.

The reload is **lazy**: nothing restores formations at boot. A formation
whose row still says `status = "active"` is not re-materialised when
`springtaled` restarts; it has to be redeployed, and only that redeploy
pulls the mental model, momentum row and rally tokens back out of the
store.

Storage key: `mental_model:<formation_id>`.
Owner: `crates/springtale-cooperation/src/mental_model/store/`.

### 2. Global cross-formation knowledge store (G2)

When a formation dissolves, the bot writes an `OutcomeNote` to the
global `GlobalKnowledgeStore`:

```
OutcomeNote {
  formation_id, intent, peak_tier, connectors,
  success_count, failure_count, dissolve_reason, at
}
```

When a *new* formation spawns, the lifecycle hook queries the global
store with `RetrievalQuery { intent, connectors }` and seeds the new
formation's `domain_knowledge` with the top-5 most-relevant prior
outcomes. Each seeded entry is keyed `prior_outcome::<old_id>` and
includes the relevance score the ranker assigned.

Storage key: `memory:outcome:<formation_id>` in the config_store
table (encrypted at rest by the same vault layer).

Owner: `crates/springtale-cooperation/src/memory/`.

## Relevance ranking

The default in-process scorer (`memory::store::score_against`) ranks
prior outcomes by:

```
score = 0.6 × intent_variant_match + 0.4 × connector_jaccard_overlap
```

- **Intent variant match** — 1.0 if the new formation's intent
  *variant* matches the prior outcome's (e.g. `Execute` ↔ `Execute`),
  0.0 otherwise. Variant comparison ignores payloads (`plan_id`,
  `reason`) so different runs of the same intent type cluster
  together.
- **Connector Jaccard overlap** — `|intersect| / max(|new|, |prior|,
  1)`. If the new formation uses {slack, github} and the prior used
  {slack, telegram, github}, overlap = 2/3 ≈ 0.67.

A future Qdrant Edge + fastembed-rs backend can drop in behind the
same `GlobalKnowledgeStore` trait — call sites depend on
`retrieve_relevant`, not on the specific scoring algorithm.

## What's *not* persisted

- **Per-tick scratch state.** Awareness snapshots, attention load,
  rally tokens — these reset every formation.
- **Active task state.** Tasks on the blackboard are scoped to one
  formation; dissolve drops them.
- **Member identities.** The new formation gets fresh `AgentId`s
  (Bitsquid handles), not the prior formation's. Knowledge about
  specific agents doesn't transfer — only knowledge about the
  *domain*.

This is intentional: the global store preserves what's
*generalisable* (which intent + connector combinations worked, what
dissolve reasons recurred), not what's incidental (a particular
agent's UUID).

## Operator surface

The mental model is mostly automatic. You don't typically configure
it. But you can:

### Inspect a formation's current model

```bash
springtale memory audit --formation <id>
```

Dumps the `SharedMentalModel` shape: counts of domain entries,
patterns, conventions; the top 10 entries by confidence.

### Compact the model

If a formation accumulates noise (lots of low-confidence entries),
compact the model to drop the bottom 50%:

```bash
springtale memory compact --max-entries 100
```

This applies to the *currently selected* formation (set via UI) or
globally if invoked without a selection.

### Query prior outcomes

```bash
springtale memory recall --intent execute --connectors slack,github
```

Lists the top-5 prior outcomes that would be seeded into a new
formation with this intent + connector set. Useful for debugging
"why did the new formation behave like the prior one?"

### Reset

```bash
springtale memory wipe --formation <id>
```

Drops both the per-formation model and the formation's outcome note
(if any). Use this when you want a clean slate.

## When the model misleads

The model is **not** consulted at every tick — it influences
orchestration decisions at Fever tier and seeds new formations at
spawn time. If a formation behaves oddly:

1. Check `springtale memory audit` for high-confidence domain entries
   the formation is over-anchoring on.
2. If a seeded `prior_outcome::*` entry is steering the formation
   into a bad pattern, `compact` it or drop confidence below 0.3
   (entries below the relevance cutoff are ignored).
3. Recovering from a sustained-bad-pattern run: `memory wipe` the
   formation, dissolve it, spawn a replacement.

## See also

- [cross-formation.md](cross-formation.md) for how *running*
  formations share state (different mechanism: chitchat gossip, not
  durable storage).
- [intervention.md](intervention.md) for how the mental model feeds
  intervention decisions.
- `crates/springtale-cooperation/src/memory/` and
  `crates/springtale-cooperation/src/mental_model/` for the
  implementation.
