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

The fastest path is the `deploy-team` operation via the API:

```bash
curl -sS -X POST http://127.0.0.1:8080/formations/deploy-team \
    -H "Authorization: Bearer $(springtale api token print)" \
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

Interference (two agents writing the same key, one undoing another's
work) drops the tier. Idle formations decay.

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

Each ships as a `springtale new <template>` starter: `cli-runner`,
`llm-swarm`, `telegram-bot`. See [`templates.md`](templates.md) for the
full 14-template menu.

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
2. **Reassign attention.** The attention broker (Army of Two aggro
   model) shifts load away from the weakest agent toward operational
   peers.
3. **Retry.** The next tick runs with the new attention distribution.
4. **Repeat or escalate.** If rally tokens reach zero, the formation
   escalates to the orchestrator. The orchestrator can `change_intent`,
   `dissolve`, `escalate` (route to a human via the autonomy gate), or
   `inject_fuel`.

Manual rally (`POST /formations/{id}/rally` or the canvas RALLY button)
runs the same choreography but skips the cascade detector — useful when
an operator sees trouble the supervisor hasn't classified yet.

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
otherwise permit them — Dissolve, force-Rally, ChangeIntent, and member
removal all require the guard to be off first. Guard surfaces in the
canvas as a badge on the formation detail card and is toggled via
`POST /formations/{id}/toggle-guard`.

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
