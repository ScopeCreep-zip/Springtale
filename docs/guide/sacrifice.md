# Sacrifice

Sacrifice is the rarest cooperation primitive: an agent voluntarily
yields the task it was about to claim to a peer it judges will do
better with it. The agent that yields takes a small attention-load
penalty in exchange for the formation's overall throughput going up.

The spec lives in
[`docs/intended-arch/COOPERATION.md §24`](../intended-arch/COOPERATION.md).

## When sacrifice fires

An agent considers sacrifice once per tick, as the **last step** of
its scan loop, after it's already picked a candidate task. The
sacrifice evaluator (`crates/springtale-cooperation/src/sacrifice/scorer.rs`)
runs at Hot tier or above and asks: "would yielding this task to a
specific peer raise the formation's expected utility more than me
executing it myself?"

The evaluator is voluntary, big-brain utility AI. It computes a
weighted score from four factors:

| Factor | Meaning |
| ------ | ------- |
| Peer attention load | Yielding to a more-loaded peer is *worse*; the evaluator favours less-loaded peers. |
| Capability fit | Yielding to a peer with stronger capability for the task is *better*. |
| Formation rally budget | When rally tokens are low, sacrifice is more attractive (cheap recovery vs expensive rally). |
| Member count | Sacrifice is only worth it in formations large enough that the load-distribution effect matters; the evaluator down-weights at N < 3. |

If the weighted score exceeds a threshold (currently 0.55 — tunable
via `sacrifice::scorer::DEFAULT_THRESHOLD`), the agent returns a
`SacrificeAction::Yield { sacrificer, beneficiary, utility }` and the
executor drops the chosen task without claiming it.

## What sacrifice is *not*

- **Not a generic "give up" mechanism.** Agents that give up because
  they're degraded use the recovery path (§18), not sacrifice.
- **Not a rally trigger.** Rally fires on cascade detection; sacrifice
  fires on per-tick utility comparison.
- **Not under operator control.** You can't manually fire a sacrifice
  — it's purely the agent's voluntary decision. (You can adjust the
  weights / threshold globally via config, but not per-decision.)
- **Not silent.** Every sacrifice emits a `SacrificeYield` event on
  the cooperation event bus, which the EventRibbon surfaces as a
  4-second green toast.

## Reading sacrifice events

The EventRibbon toast looks like:

```
SACRIFICE YIELD
abc12345 → def67890 (utility 0.72)
```

The first id is the sacrificer (who gave up the task), the second is
the beneficiary (who's expected to pick it up next tick), and the
parenthesised number is the utility score that crossed the threshold.

In the BottomPanel formation log, the same event renders compactly:

```
12:04:31  YIELD  abc12345 → def67890 (0.72)
```

## When you want *more* sacrifice

Increase formation member count. Sacrifice down-weights at N < 3; at
N ≥ 5 the load-distribution effect dominates and you'll see regular
yields when peers are unbalanced.

You can also lower the threshold via the formation's
`SacrificeConfig` (currently exposed only programmatically; a UI
control is on the Phase 2b backlog). Threshold = 0.4 makes sacrifice
quite chatty; 0.7 makes it rare.

## When you want *less* sacrifice

Raise the threshold or reduce member count. The dominant cause of
"too much sacrifice" is a formation with one heavily-loaded scout
agent and several lightly-loaded support agents — the scout
sacrifices repeatedly because the peer fit is good and the load
delta is large. Either:

- Add more scouts (so the load is distributed at the source).
- Demote the support agents (so the peer-fit score drops below
  threshold).
- Raise the threshold to 0.7+ if you specifically want sacrifice as a
  rare "emergency only" mechanism.

## Sacrifice and rally

These two interact at Hot+ tier. The sacrifice scorer reads the
formation's remaining rally tokens; when tokens are low (≤ 1), the
score weights *up* (sacrifice is cheaper than burning rally). When
tokens are full (3 of 3), the score weights *down* (rally is plenty,
no need to yield).

Practically, this means a formation that exhausts rally then enters
Hot tier will see more sacrifices as the formation tries to recover
through redistribution before falling into intervention.

## Common questions

**My agent never sacrifices. Is something broken?**

Probably not. Sacrifice is rare by design. Check tier: at Cold or
Warming the evaluator short-circuits to None. Check member count: at
N < 3 the formation-size weight is zero.

**Sacrifice fires every tick. That seems wrong.**

Likely the threshold is too low or the formation is severely
unbalanced. Inspect the EventRibbon stream: if every yield names the
same beneficiary, that peer is consistently the best-fit candidate
and the rest of the formation should probably transform roles to
match.

**Does sacrifice leak the task forever?**

No. The yielded task stays on the blackboard with `assigned_to =
None`; the beneficiary picks it up on its next tick. If the
beneficiary also yields, you get a hot-potato pattern — the
interference detector flags this and the supervisor will eventually
fire `Escalate`.

## See also

- [intervention.md](intervention.md) for what happens when sacrifice
  isn't enough.
- [`docs/intended-arch/COOPERATION.md §24`](../intended-arch/COOPERATION.md)
  — formal sacrifice spec.
- `crates/springtale-cooperation/src/sacrifice/scorer.rs` — the
  weighted-sum implementation and threshold constants.
