# Interference Detection — Decision Log

Interference detection per `COOPERATION.md §13`. Three classes of
interference are detected on every tick, sliced from the
`SharedEnvironment` write log into "history" (writes from earlier ticks)
and "records" (writes from this tick) so Lamport ordering is honored:

- **TaskAlreadyClaimed** — two agents racing for the same blackboard task.
  Detected via the CAS path in `atomic_cas.rs`.
- **DuplicateAction** — two agents producing the same `ActionDescriptor`
  payload-hash within the same tick window.
- **ActionNegation** — one agent's action semantically reverses another's
  (e.g. open then close the same connector binding).

## Decision: in-house detector, not `automerge` (E5 audit-fix)

The cooperation spec floated `automerge` 2.x as a substrate for
ActionNegation detection because automerge models causal history with
Lamport-style timestamps. The validation pass surfaced two reasons that
path doesn't pay off here:

1. **`automerge` 3.x dropped major-memory cuts** — what the spec
   referenced (low per-doc memory) no longer holds in 3.x. We'd take a
   regression for the same semantic capability.
2. **Cross-key semantic ActionNegation isn't first-class in automerge.**
   automerge tracks per-key conflict resolution (last-writer-wins,
   merge-tactics). What we need is "operation A and operation B are
   semantic inverses" which requires a domain-specific predicate
   (`negation_pairs.rs`). automerge would be a heavy dep that doesn't
   actually do the work.

Conclusion: keep `interference/detector.rs` as the canonical detector.
It walks the shared-env write log directly, applies the negation-pair
predicates from `types.rs`, and emits `Interference` records consumed by
`tick_steps/log_interference.rs`. No `automerge` dep added.

## Hot path

- `tick_steps/build_reports::run` calls
  `tick_processor::process_tick_with_context` with the new-this-tick
  records and the historical write log slice.
- `tick_processor` calls into `interference::detector::detect_*`.
- Interferences are folded into the `FormationTickResult` and surface in
  the per-member `TickReport.interference_with` for the awareness step.

## Where the predicate lives

`interference/types.rs::action_negates` is the per-pair predicate. Adding
a new connector means adding (at most) one negation pair if the
connector has a "do/undo" action surface (e.g. `enable`/`disable`,
`subscribe`/`unsubscribe`). Most connectors don't need entries.
