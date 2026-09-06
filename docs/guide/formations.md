# Building Multi-Agent Formations

A **formation** is a group of bots (agents) cooperating on a shared
intent. It's the unit of coordination: one formation, one intent, many
agents. Formations have momentum (a tier from Cold → Fever that
unlocks more advanced capabilities), rally tokens (Monster Hunter
quest-fail carts), and a shared environment all members can read.

This guide is task-oriented. For the conceptual model, read
[cooperation.md](cooperation.md).

## When to use a formation (vs. a single bot)

Use a **single bot** when:

- One agent can do the whole task.
- You don't need cross-agent consensus or handoff.
- The task is stateless.

Use a **formation** when:

- The task benefits from specialization (researcher + writer + critic).
- You want redundancy — one agent fails, others rally.
- Multiple agents need to agree before acting (consensus gating).
- You want to observe emergent behavior.

## Deploy a formation from the CLI

The fastest path is the `deploy-team` operation via the API. It needs
a bearer token the daemon issued — `springtale login` exchanges your
vault passphrase for one and stores it for the CLI (mode 0600), so
every `springtale` command authenticates by itself afterwards:

```bash
springtale login            # exchange the passphrase for a token
springtale auth tokens      # the tokens the daemon has issued
# springtale auth revoke <id>   # withdraw one you no longer trust
```

For a raw `curl`, point `SPRINGTALE_API_TOKEN` at a token you hold —
the same variable the CLI reads before it falls back to the stored one:

```bash
curl -sS -X POST http://127.0.0.1:8080/formations/deploy-team \
    -H "Authorization: Bearer $SPRINGTALE_API_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
          "name": "Research Squad",
          "intent": "Reconnoiter",
          "guard_mode": false,
          "agents": [
            {
              "connector_name": "connector-telegram",
              "trigger_name": "message_received",
              "action_connector": "connector-telegram",
              "action_name": "send_message"
            }
          ]
        }'
```

One call creates the rules, the formation, and marks the formation
active. The endpoint is atomic — if any rule fails validation, all
rules roll back.

## Intents

A formation always has exactly one intent. Cycling intents is cheap
and encouraged:

- `Reconnoiter` — monitor, read-only.
- `Execute` — take action.
- `Stabilize` — maintain current state.
- `Surge` — maximum effort, burn rally tokens freely.

Cycle via `POST /formations/{id}/cycle-intent` or the colony canvas.

## Momentum tiers

Formations start at `Cold`. Successful cooperation drives the tier up:

| Tier | Threshold | Unlocks |
|------|-----------|---------|
| Cold | (start) | Read-only environment |
| Warming | ≥3 successful ticks | Neighbor gossip + basic chaining |
| Hot | ≥8 successful ticks, 0 interference | Environment writes + synchronized commit |
| Fever | ≥15 successful ticks, 0 interference | Consensus + AI orchestration + recruit |

Only ticks that did real work count — an idle tick is not a success.
Interference (two agents writing the same key, one undoing another's
work) resets the combo: the tier drops and the formation starts
climbing again from there. Idle formations decay one step per decay
interval, not straight to Cold.

The tier isn't a slider you set — it's a consequence of how well the
formation cooperates.

## Rally tokens (when something goes wrong)

Each formation starts with 3 rally tokens (Monster Hunter default). A
rally is consumed when a member fails and the formation self-heals by
redistributing attention. Run out of tokens → the formation escalates
to the orchestrator.

Manually trigger via `POST /formations/{id}/rally`. Inspect remaining
tokens via the BottomPanel rally pips or `GET /formations/{id}`.

## Inspecting a live formation

The colony canvas (`/dashboard`) renders every live formation as a
dashed ellipse with momentum-colored members inside. Click the ellipse
to see detail: momentum counters, rally pips, guard status, member
health + liveness icons, attention distribution bar (Army of Two aggro
meter), and active task per member.

The structured JSON is at `GET /formations/{id}`:

```json
{
  "id": "...",
  "name": "Research Squad",
  "intent": "Reconnoiter",
  "status": "active",
  "member_count": 3,
  "operational_count": 3,
  "momentum_tier": "Warming",
  "momentum_label": "WARM",
  "momentum_consecutive_successes": 5,
  "momentum_interference_count": 0,
  "momentum_successes_to_next_tier": 3,
  "rally_tokens": 3,
  "rally_max": 3,
  "member_details": [
    {
      "agent_id": "...",
      "connector_name": "connector-telegram",
      "role": "General",
      "health": {"type": "Operational"},
      "attention_load": 0.33,
      "liveness": "Alive"
    }
  ]
}
```

## Dissolving

Formations dissolve gracefully: `POST /formations/{id}/dissolve`. The
dissolve runs through the cadence tick loop once more to persist final
state, then drops. Members that were exclusively in this formation
become free; members in multiple formations keep running.

The dissolve also publishes a terminal outcome — on the formation gossip
bus and, when one is wired, into the cross-formation knowledge store. Both
carry `success_count` (the momentum FSM's `consecutive_successes`) and
`failure_count`, which is the momentum FSM's lifetime `interference_total`.
Retrieval scores priors on `success_count / (success_count + failure_count)`,
so the failure side has to be the real count.

## Persistence

Formation *state* persists — membership, intent, guard state, momentum,
rally tokens and the shared mental model are written to the store — but a
Formations are **restored at boot**. `springtaled`'s
`runtime::boot::formations::restore_formations` lists stored formations after
`init_bot` returns and re-issues the same `FormationCommand`s the API would
send: `Deploy` for every row that was `active` or `paused` when the daemon
last stopped, then `Pause` for the ones that were paused. That deploy is what
reads the persisted state back in (`lifecycle::spawn_formation`) — momentum
row, rally tokens and the shared mental model. A formation whose connectors
are not installed still restores; it spawns with missing capabilities and
reports that through the normal liveness path. Only a row whose id will not
parse as a `FormationId` is skipped, with a `warn`. Autonomy is keyed by
rule id (or set for a whole formation), never by name; a member without
an explicit setting runs at act-autonomously.

## Worked examples

The three worked examples from
`docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md` §7 illustrate
three concrete formation shapes:

- **CLI task runner** (§7.1) — 3 workers on one task, no AI. Showcases
  cadence broadcast + rally.
- **LLM orchestration swarm** (§7.2) — researcher + writer + critic
  with handoff between them. Showcases momentum progression + handoff.
- **Telegram bot** (§7.3) — responder + memory-keeper + moderator,
  formation per incoming message. Showcases consensus gating.

Each ships as a recipe: browse the library in the colony UI or see
[`recipes.md`](recipes.md). `springtale init` gives you the bare
project; a recipe fills it in.

## Advanced topics

### The blackboard

Every formation owns a shared key/value workspace called the blackboard.
Members read and write to it without sending direct messages — that's
the **stigmergic** style of coordination, the same model insects use to
build a hive. The live store lives in
`crates/springtale-bot/src/cooperation/blackboard/`; types and write-log
semantics live in `crates/springtale-cooperation/src/state/`.

Two rules matter:

- **Writes carry a trace id.** Every blackboard write records the agent
  id and the tick it landed on. Consumers can compose causally — "use
  the result the writer agent produced this tick".
- **The write log is split per tick.** A Lamport-style
  `last_tick_write_count` cursor splits writes into "before this tick"
  and "this tick" so the interference detector can run cleanly without
  re-scanning history.

Sub-tasks posted by the orchestrator (Fever tier) live under the
`task:*` key prefix; the agent-loop `scan` step pulls from there.

### Interference detection

When two members take conflicting actions inside the same tick, the
tick processor records an interference event. Four kinds:

| Kind | Example |
|---|---|
| Resource conflict | Two agents claim the same `ResourceId` |
| Action negation | One agent's write undoes another's (detected via the write-log diff) |
| Collateral damage | A side-effect of one action harms another's working state |
| Redundancy | Two agents do the same work without coordinating |

Interference events feed the momentum update in step 4 (a tick with
interferences does *not* count as a successful tick). Persistent
redundancy or negation is what causes the supervisor to swap an
agent's role in step 10.

### Rally choreography

When the supervisor detects cascade risk (two failures inside the
recent window, or a single failure on an agent the formation depends
on), it triggers rally. The choreography:

1. **Burn a token.** `rally_tokens` decrements by one. UI updates the
   pip count.
2. **Reassign attention.** `attention.release(agent, 0.2)` shifts 0.2 of
   the failing agent's share of the zero-sum attention economy (Army of
   Two aggro model) out to every other member, so peers absorb the load.
   Momentum takes a recorded failure at the same time.
3. **Retry.** The next tick runs with the new attention distribution.
4. **Repeat or escalate.** If rally tokens reach zero, the formation
   escalates to the orchestrator. The orchestrator can `change_intent`,
   `dissolve`, `escalate` (route to a human via the autonomy gate), or
   `inject_fuel`.

Manual rally (`POST /formations/{id}/rally` or the canvas RALLY button)
runs the same choreography but skips the cascade detector — useful when
an operator sees trouble the supervisor hasn't classified yet.

**Known defect.** The manual path picks its target with `min_by` on
attention load (`tick_steps/handle_command.rs`), i.e. the *least* loaded
operational member, and then shifts 0.2 further away from it. The cascade
path passes the actually-failing agent. So a manual rally moves load off
whichever member is already doing the least, which is the opposite of what
the choreography above is for.

### Custom roles

Out of the box, agents take one of three dynamic roles:

- **General** — full task pickup, full action authority.
- **Info** — observation only; posts to blackboard, never writes outside.
- **Support** — augments other members (boosts their attention budget,
  takes on their interferences) but doesn't claim primary tasks.

The role is stored on the agent and updated by step 10 in response to
supervisor decisions. To define a custom role, implement
`DynamicRoleTrait` from `crates/springtale-cooperation/src/role/` and
register it via the runtime — typical case is when you want a
domain-specific role (e.g. "Translator" or "Reviewer") with bespoke
attention rules.

### Guard status

Every formation has a guard toggle. When guard is **engaged**, the
formation refuses destructive actions even if the autonomy level would
otherwise permit them — dissolve, intent change, member removal, recruit
and rally are each refused with an error while the guard is on, and
nothing changes; disengage the guard first. A guarded formation also
refuses synthesized actions classified `Destructive`
(`operations/formation_synthesis.rs`). Guard surfaces in the canvas as a
badge on the formation detail card and is toggled via
`POST /formations/{id}/toggle-guard`.

The toggle is live. `operations::config::toggle_formation_guard` writes
the durable `guard:{formation_id}` config row *and* posts
`FormationCommand::SetGuard`, which the bot applies to the running
formation's `constraints.guard_mode` on its next command drain — the same
channel dissolve, pause and intent change ride. Deploy seeds the live flag
from the same row (`lifecycle::spawn_formation`), and every reader of the
row goes through `config::formation_guard_engaged`, so the badge, the API
and what the running formation actually refuses cannot disagree, with or
without a redeploy.

The intent is to make accidental destruction harder: a formation that
just hit Fever and is producing useful output is exactly the one you
don't want to lose because a slash command mis-targeted it.

### Intent drives automation (rule synthesis)

A formation **synthesises persistent rules from its intent** — with or
without AI. When you deploy a team, each agent's `(connector, trigger,
action)` is stored as the formation's *automation config*, and rules are
derived from it and scoped to the formation (`RuleOwner::Formation`):

- **Reconnoiter** (monitor, read-only) → each trigger fires a read-only
  observation (`Notify`); the mutating action is never invoked.
- **Execute** / **Surge** (take action) → each trigger fires its
  configured `RunConnector` action. Under guard mode, an action the
  sentinel classifies as destructive is downgraded to an observation.
- **Stabilize** (maintain) → observation only.
- **Dissolve** → no rules.

Cycling the intent (`POST /formations/{id}/cycle-intent`) re-synthesises
the rules for the new intent. This is **non-lossy**: the canonical action
lives in the automation config, so flipping Reconnoiter → Execute → back
restores the exact action. The work is deterministic — a formation with
`NoopAdapter` (no AI) produces outward effect on its own; an attached AI
adapter at Fever tier *additionally* proposes richer, parameterised
subtasks on top.

In practice this means:

- A paused-and-resumed formation keeps both its cadence/momentum state
  **and** its synthesised rules.
- Dissolving a formation tears down its rules and automation config.
- Formation-scoped rules fire only in their formation's context (a
  formation A rule never fires for formation B), and they execute with
  the formation's momentum tier so sentinel / autonomy gating applies
  correctly.

The live (per-tick) counterpart is the deterministic decomposer in
`springtale-bot` `orchestrator::orchestrate::decompose_intent_deterministic`,
which mechanically derives read/poll subtasks from member capabilities;
the persistent counterpart is `springtale-runtime`
`operations::formation_synthesis`.
