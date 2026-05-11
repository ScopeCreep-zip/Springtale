# Consensus

Consensus is Springtale's vote primitive: when an agent wants to do
something destructive at Fever tier, it opens a vote on the formation's
`ConsensusEngine` and the other operational members ballot before the
action proceeds. The vote either approves (action executes), denies
(action drops), or times out (action drops with "no quorum" recorded).

The spec lives in
[`docs/intended-arch/COOPERATION.md §11`](../intended-arch/COOPERATION.md);
this page covers when votes open in practice and what you can do as
the operator.

## When a vote opens

A consensus vote opens **only** at Fever tier and **only** for actions
classified `ApprovalPolicy::RequireConsensus`. That classification is
declared in the connector manifest: a connector marks specific actions
as needing consensus, and the cooperation runtime gates them
accordingly.

Practically, that means:

- An action with `ApprovalPolicy::AlwaysRequire` opens a vote at *any*
  tier (this is the rare "high-stakes everywhere" case).
- An action with `ApprovalPolicy::RequireConsensus` opens a vote at
  Fever tier only — lower tiers fall back to the autonomy gate.
- An action with `ApprovalPolicy::None` never opens a vote.

The agent that picks the task calls `formation.consensus.propose(...)`
under the borrow on `formation.members`; once the `&mut` borrow
releases, the proposal is queued and the cooperation tick emits a
`ConsensusVoteOpened` event.

You see it as:

- **EventRibbon:** not surfaced (votes aren't high-severity).
- **BottomPanel event log:** `VOTE OPEN <id>` with the deadline.
- **Tick log:** `tracing::info!` line tagged with `vote_id`.

## Casting ballots

Every operational member of the formation is a voter. Members cast
ballots either through:

1. **Default policy** — autonomy-driven. A member at Autonomous
   autonomy votes "approve" by default for actions in its capability
   set; a member at Suggest votes "deny" by default for destructive
   actions. The default-policy machinery lives in
   `crates/springtale-cooperation/src/consensus/vote.rs`.
2. **Operator override** — you can override a member's default via
   the CLI: `springtale agent <id> vote <vote_id> approve` or `deny`.
   Override tokens (`override_budget`) are scarce: each agent starts
   with 1 per formation lifetime by default. Once used, the agent
   reverts to default-policy voting.

## When a vote resolves

The cooperation tick's `consensus_deadlines` step runs each tick and
resolves any vote whose deadline has elapsed. Three outcomes:

| Outcome | What happens |
| ------- | ------------ |
| `Approved` | The action executes. `ConsensusVoteResolved { outcome: "approved" }` fires; the agent picks up the task on the next tick. |
| `Denied` | The action drops. The agent's `chosen_task` is cleared and the tick records a `ConsensusVoteResolved { outcome: "denied" }` event. |
| `Timeout` | Treated identically to `Denied` for the action, but the event distinguishes `outcome: "timeout"` so observability can flag the formation as under-quorum. |

Default deadline: 5 seconds. Specific actions can override via the
manifest if their consensus window needs to be tighter or longer.

## How to read vote events

The BottomPanel's formation event log shows two related events per
vote:

```
12:04:31  VOTE OPEN  abc12345 5000ms
12:04:36  VOTE END   abc12345 → timeout
```

Severity-coloured left borders mirror the cooperation event taxonomy:

- **Yellow:** vote open (pending decision).
- **Green:** vote resolved (approve or deny — both are decisions).
- **Red:** never used for consensus; reserved for harder failures.

## Override budgets in practice

Each agent's `consensus.override_budget` defaults to 1. The override
budget is per-agent, per-formation: when a formation dissolves, every
agent's budget resets on the next spawn.

If you find yourself overriding more than once on the same formation,
the per-agent autonomy is probably set wrong. Switch the agent to a
higher autonomy level via `springtale agent <id> autonomy up` to
reduce the rate at which it tries destructive actions you'd want to
override.

## Common questions

**Why didn't my action need a vote even at Fever?**

The action's manifest marks it `ApprovalPolicy::None`. Voting is a
per-action declaration in the connector's manifest, not a tier-wide
default. Check the connector's `manifest()` output.

**Why did the vote time out with no ballots?**

Either every operational member already used their override token, or
the formation has only one operational member (single-voter quorum
requires the single voter to ballot). Add members or reset the
override budgets (dissolve + re-spawn).

**Can I see who voted what?**

Yes — the consensus event log records each ballot. Query via the SSE
stream `/cooperation/events?formation_id=<id>` and filter to
`vote_ballot` (when implemented) or read the per-tick log from
`tracing` output. The full ballot history is also persisted to the
mental model on dissolve.

## See also

- [intervention.md](intervention.md) for what happens when a denied
  vote leaves the formation stuck.
- [`docs/intended-arch/COOPERATION.md §11`](../intended-arch/COOPERATION.md)
  — formal consensus specification.
- `crates/springtale-cooperation/src/consensus/` — implementation,
  including the default-policy machinery and override-budget
  bookkeeping.
