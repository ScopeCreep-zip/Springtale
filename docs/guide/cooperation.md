# Cooperation

Springtale is an RTS game engine that happens to run bots. The
`springtale-cooperation` crate is where that framing lives: 42 pub modules
modelling how peer agents coordinate on a shared intent without a central
controller. This page is a user-facing tour.

The full specification (with game-engine provenance for every pattern) is
[`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md). The
wiring into the bot runtime is in [architecture.md §6](architecture.md).

## 1. Where it lives

```
crates/
├── springtale-cooperation/     the crate — 42 pub modules, zero internal deps
│   └── src/
│       ├── cadence.rs          tick bus
│       ├── momentum.rs         tier state machine
│       ├── rally/              self-healing before escalation
│       ├── recovery/           distress → helper
│       ├── supervision/        Erlang OTP + K8s liveness probes
│       ├── stigmergy/          ambient signalling
│       ├── contract_net/       FIPA CNP task allocation
│       ├── interference/       conflict detection
│       ├── mental_model/       shared learned knowledge
│       ├── transformation/     role changes on capability loss
│       ├── pacing/             work/rest (L4D Director)
│       ├── handoff/            work-product transfer
│       ├── consensus.rs        weighted voting
│       ├── commit.rs           synchronised execution barriers
│       ├── attention/          Army of Two aggro economy
│       ├── awareness/          neighbour perception (within-formation)
│       ├── gossip/             cross-formation event bus (FormationView)
│       ├── memory/             persistent shared memory (between blackboard + mental_model)
│       ├── capability/         tier-aware capability projection
│       ├── authority/          momentum × layer permission matrix
│       ├── routing/            L1/L3 task routing
│       ├── role/               DynamicRoleTrait (General/Info/Support)
│       ├── dissemination/      L2 state dissemination
│       ├── comms/              inter-agent messaging bus
│       ├── sacrifice/          deliberate self-cost
│       ├── state/              shared environment (workspace, snapshot)
│       ├── agent/              per-agent 5-step loop
│       ├── tick_processor.rs   per-tick interference aggregator
│       ├── utility/            utility AI scoring (big-brain)
│       ├── replan/             CBBA global re-plan
│       ├── action.rs           SubTask / SubTaskResult (claim → result)
│       ├── action_state.rs     per-tick action lifecycle state
│       ├── peer.rs             PeerMsg — direct inter-agent message envelope
│       ├── layer.rs            7-layer routing abstraction
│       └── types.rs            FormationId, AgentHealth, etc.
│
└── springtale-bot/
    ├── src/cooperation/        the glue — live Formation, Blackboard,
    │                           FormationMember (owns runtime fields
    │                           like active_task, fuel, capabilities)
    └── src/runtime/tick_steps  the 25-step tick pipeline
```

The crate itself has zero dependencies on other Springtale crates. The live
`Formation` struct (with mutable runtime fields like `active_task`,
`fuel`, `liveness`) lives in `springtale-bot` — only the bot has access to
the types that mutate per-tick.

## 2. The two loops

Cooperation happens at two scales:

**Per-agent loop (4 steps).** Every formation member runs this each tick,
in this order (`springtale-cooperation::agent::loop_`):

```
  sense  →  inbox  →  react  →  scan
```

- `sense` — read local awareness and neighbour snapshots (L0)
- `inbox` — process direct messages and handoffs (L3)
- `react` — react to stigmergy surfaces, awareness only (L2)
- `scan` — pull from the task router (L1 routing) and blackboard

`respond_cfp` is *not* a fifth step in this loop. Bidding on an open
Contract Net Protocol (L4) fires reactively from the runner when a call
for proposals arrives, not on a fixed position in the tick.

**Per-formation tick (25 steps).** The bot event loop runs this for each
active, non-paused formation when the cadence bus fires. See
[architecture.md §6](architecture.md#6-the-cooperation-tick) for the full
map.

## 3. Momentum

A formation's coherence is tracked as a four-tier state machine:

| Tier | Unlocked |
|---|---|
| Cold | Read shared environment. Neighbour awareness is off. |
| Warming | Write to the blackboard. Neighbour awareness turns on. |
| Hot | Commit barriers. Consensus voting with simple majority. |
| Fever | AI orchestration. Orchestrator decomposes intent into sub-tasks. |

Tier transitions are driven by consecutive successes and interference
events. An idle tick does not count as a success. An interference resets
the combo and the formation climbs again from there. Inactivity decays
the tier one step per decay interval. Momentum is persisted to the
`formation_momentum` table every tick and survives a daemon restart — the row is
read back when boot redeploys the formation.

Promotion and demotion are decided by rates over the current clean run,
not by how many ticks have passed. Each tick adds its counts (actions,
successes, duplicate actions, handoffs) to a `RunWindow`; the next tier
opens when the window clears its row of the `[cooperation.momentum]`
table, and once the window holds the current tier's minimum actions a
success rate under that row or a duplicate rate over it drops one step and
restarts the window. Rows: Cold → Warming at 3 actions, 80 % success, any
duplicate work; Warming → Hot at 8 actions, 90 % success, at most 30 %
duplicate actions; Hot → Fever at 15 actions, 95 % success, at most 10 %
duplicate actions. There is no interference rate in the table: an
interference restarts the run (and breaks Fever), so it is always zero at
promotion time. A failed tick also breaks Fever; below Fever it only
lowers the run's success rate. Two members doing the same work every tick
therefore never reach Hot. These numbers are Springtale's own starting
values, not Overcooked-AI's or any game's; they are configuration so they
can be tuned after play. `consecutive_successes` is still tracked and
persisted for the UI hint but no longer decides promotion.

## 4. Rally, recovery, supervision

When a formation struggles, the tick pipeline tries to self-heal before
escalating to the orchestrator.

**Rally.** Detected cascade risk → burn a rally token → redirect attention
to the weakest agent → try again. Rally tokens are finite; when they run
out, the formation escalates. In the UI, rally tokens appear as pips
(Monster Hunter cart icons).

**Recovery.** Distressed agents (Degraded, Incapacitated, Dead-recoverable)
emit a `DistressSignal`. Each operational peer evaluates whether to help
based on capability match, own attention load, and proximity. The first
willing helper claims the rescue. First-willing-wins is per the L4D rule
for pinned-survivor rescue.

**Supervision.** Per-member liveness probe (Alive / Suspect / Down,
Kubernetes-style) combined with an Erlang OTP supervisor tree. For each
member the supervisor can emit `TransformRole`, `RetryWithRally`,
`TriggerReplan`, `MarkDown`, or `Escalate`.

## 5. Interference

Two agents taking conflicting actions in the same tick. Four kinds:

- **Resource conflict** — both claim the same `ResourceId`
- **Action negation** — one action undoes another (Lamport-split write log)
- **Collateral damage** — side-effect harms a peer's work
- **Redundancy** — two agents do the same work

Interference events feed momentum updates (a tick with interferences does
not count as a success) and are logged for observability.

## 6. The environment

Formations share a blackboard (key-value workspace) with a write log.
Writes are CAS-style — every write carries a trace id. Sub-tasks are
posted under the `task:*` key prefix so `scan_tasks()` finds them.

Stigmergy sits on top of the shared environment: agents mark surfaces
(e.g. "handled GitHub issue #123 at tick 4217"); peers perceive the marks
and adjust behaviour without direct messaging. Surfaces decay over time.

## 7. Mental model

Each formation accumulates a `SharedMentalModel`: learned domain
knowledge, successful cooperation patterns, shared vocabulary, project
conventions. Updated every tick from observed reports and interferences
(step 13). Persisted across formation dissolves to the `mental_model_*`
tables (`crates/springtale-store/src/schema/sql/cooperation.sql`) so
later formations with the same id benefit from what prior instances
learned.

## 8. Autonomy

Each agent has an autonomy level (0 A.D. stance system), stored
per-agent:

| Level | Behaviour |
|---|---|
| Observe | Never acts. Posts observations for a human operator. |
| Suggest | Proposes claims but does not act on them. |
| Approve | Claims tasks and executes after approval. |
| Autonomous | Claims and executes without approval. Default for a rule or member with no explicit setting. |

The agent loop's `decide_agent_tick` function gates the action path on the
configured autonomy level. Autonomy is keyed by rule id, or set for a
whole formation (`POST /formations/{id}/cycle-autonomy`) — never by
name. Change it from the bottom panel in the colony canvas or with
`springtale agent set-autonomy <rule> <level>` (a rule name or id; the
CLI resolves a name to its id).

## 9. Tool calls

AI adapters emit `ToolCall` structs; `springtale-bot::tool_runner` routes
them back through the same capability gate that guards direct actions.
Supported by all three adapters (Anthropic, Ollama, OpenAI-compat). The
MCP server exposes connectors as tools to external AI callers through the
same path — there is no back door around the capability system.

## 10. What's surfaced in the UI

The colony canvas shows live cooperation state:

- Formation zone glyph encodes `status` (draft / active / paused)
- Rally pips render remaining rally tokens
- Guard-status badge shows whether guard mode is engaged
- Attention bar renders zero-sum load distribution across members
- Per-member health state (Healthy / Degraded / Incapacitated / Dead)
- Per-member liveness (Alive / Suspect / Down) via opacity + icons
- Momentum tier shown on the formation detail card
- Intent shown as a pattern string at the top of the detail card

See [guide/colony-canvas.md](colony-canvas.md) for the full visual
vocabulary.

## 11. Modules at a glance

The 42 pub modules of `springtale-cooperation` group cleanly into seven concerns. Use this as a map when reading the crate.

**Lifecycle and timing.**

| Module | Role |
|---|---|
| `cadence` | Tick bus (`tokio::sync::broadcast`); slow consumers drop to a lagged signal. |
| `momentum` | Cold → Warming → Hot → Fever state machine + decay. Persisted every tick. |
| `pacing` | GCRA work/rest phase transitions (L4D AI Director model). |
| `tick_processor` | Per-tick aggregator: action records + interference detection. |
| `command` | Lifecycle events (Deploy / Pause / Resume / Dissolve / ChangeIntent / AddMember / RemoveMember / Rally). |

**Shared context.**

| Module | Role |
|---|---|
| `state` | Workspace, snapshot, write log. Underlies the live blackboard in `springtale-bot::cooperation::blackboard`. |
| `context` | `FormationContext` — read-only state broadcast to members each tick. |
| `dissemination` | Step 6 broadcast layer. |
| `awareness` | Within-formation neighbour perception (chitchat or InMemory substrate). |
| `comms` | Inter-agent message bus, implicit signals, cohesion signals. |
| `peer` | `PeerMsg` — typed direct-message envelope between agents. |
| `mental_model` | Learned domain knowledge, vocabulary, conventions. Persisted across dissolves. |
| `memory` | Persistent shared memory between blackboard (ephemeral) and mental_model (durable). |

**Cross-formation.**

| Module | Role |
|---|---|
| `gossip` | Cross-formation event bus carrying `FormationView` snapshots — lets sibling formations see what their peers are doing without polling the API. See [guide/cross-formation.md](cross-formation.md). |

**Action lifecycle.**

| Module | Role |
|---|---|
| `action` | `SubTask` / `SubTaskResult` — what an agent claims and what it returns. |
| `action_state` | Per-tick state of in-flight actions (claimed / running / completed / interfered). |

**Coordination patterns.**

| Module | Role |
|---|---|
| `contract_net` | FIPA Contract Net Protocol task allocation (broadcast → bid → award). |
| `routing` | L1/L3 task router; agent `scan` step pulls from this. |
| `stigmergy` | L0 ambient signalling — agents leave/perceive surface marks. |
| `handoff` | Direct / FlexChain / sequential / informational work-product transfer. |
| `consensus` | Weighted voting with deadlines (Hot tier+). |
| `commit` | Synchronised execution barriers, expired in step 12. |

**Resilience.**

| Module | Role |
|---|---|
| `interference` | Resource conflict / action negation / collateral / redundancy detection. |
| `rally` | Cascade detection + token burn + attention redirect (Monster Hunter carts). |
| `recovery` | Distress signal → first-willing helper rule (L4D pinned-survivor). |
| `supervision` | Per-member liveness probes + Erlang OTP supervisor decisions. |
| `sacrifice` | Deliberate self-cost evaluator (consulted by recovery). |
| `transformation` | Role swap on capability loss. |

**Capability and autonomy.**

| Module | Role |
|---|---|
| `capability` | Tier-aware capability projection — what's unlocked at this momentum. |
| `authority` | Momentum × layer permission matrix. |
| `attention` | Zero-sum aggro economy (Army of Two). |
| `role` | DynamicRoleTrait (General / Info / Support). |
| `layer` | 7-layer routing abstraction. |
| `utility` | Utility-AI scoring (big-brain mode). |
| `replan` | CBBA global re-plan (orchestrator escalation only). |

**Per-agent loop.**

| Module | Role |
|---|---|
| `agent` | The 5-step loop: `sense` → `scan` → `react` → `respond_cfp` → `inbox`. |
| `types` | `FormationId`, `AgentHealth`, shared identifiers. |

The bot crate's `cooperation` module owns the live runtime: `Formation`, `FormationMember` (active task, fuel, capabilities, liveness), `Blackboard` (live store), and the `FormationCommand` channel. That separation is intentional — `springtale-cooperation` is pure logic with no internal Springtale deps; `springtale-bot::cooperation` is the only place where mutable per-tick state exists.

## 12. The tick pipeline

The pipeline is `springtale-bot::runtime::tick_steps::run_tick`, laid out
step by step in [architecture.md §6](architecture.md#6-the-cooperation-tick).
`springtale-bot::runtime::event_loop::handle_cadence_tick` is the driver: it
takes the locks, calls `run_tick` per formation, then runs the tail passes
(`tail::reclaim_dead`, `drain_member_subs`, `drain_rally_events`,
`retain_viable`). Each step is one named module under
`crates/springtale-bot/src/runtime/tick_steps/`, and most delegate to
`springtale-cooperation`:

| Step module | What it delegates to |
|---|---|
| `build_reports` | per-member agent loop, `springtale-cooperation::agent::step::*` |
| `update_momentum` | `springtale-cooperation::momentum` (decay is `formation.momentum.check_decay()` immediately before) |
| `liveness`, `supervision`, `fuel` | `springtale-cooperation::supervision`, member fuel state |
| `implicit_signals`, `state_broadcast` | `springtale-cooperation::comms` |
| `persist_momentum` | `springtale-store` → `formation_momentum` table |
| `publish_context`, `gossip_awareness` | `springtale-cooperation::dissemination`, `::awareness` (Warming+) |
| `log_interference` | `springtale-cooperation::interference` |
| `check_pacing` | `springtale-cooperation::pacing` — folds the tick's `StressSample` |
| `check_cascade`, `recovery` | `springtale-cooperation::rally`, `::recovery` |
| `check_interventions` | L6 commander override |
| `transformation` | `springtale-cooperation::transformation` |
| `replan_cbba` | `springtale-cooperation::replan::cbba` |
| `resolve_consensus`, `tick_commits`, `expire_commits` | `springtale-cooperation::consensus`, `::commit` |
| `update_mental_model` | `springtale-cooperation::mental_model::learning::update_model` |
| `orchestrate_step` | `springtale-bot::orchestrator` (Fever tier only) |
| `publish_formation_view` | cross-formation gossip bus |
| `emit_canvas_update` | `runtime.canvas_tx`; skipped when no sender is wired |

## 13. Mental model lifecycle

The mental model is the formation's long-term memory. Unlike the blackboard (per-formation, ephemeral) and stigmergy surfaces (per-formation, decaying), mental model state crosses formation dissolves — a new formation deployed against the same `formation_id` warm-starts with what prior instances learned.

**Accumulation.** Step 13 of every tick calls `mental_model::learning::update_model` with the cadence reports and interference events from the current tick. The model accumulates four kinds of fact:

- **Domain knowledge** — facts the formation has confirmed (e.g. "github webhooks for repo X arrive within 2 seconds of push").
- **Cooperation patterns** — sequences of handoffs that succeeded (e.g. "researcher → writer → critic produces good results when researcher's confidence > 0.7").
- **Vocabulary** — terms the formation has converged on for shared concepts.
- **Conventions** — implicit rules learned from observation (e.g. "we don't post during 22:00–06:00 local time").

**Persistence.** On `Dissolve`, the bot crate writes the mental model to the `mental_model_*` family of tables (`crates/springtale-store/src/schema/sql/cooperation.sql`) keyed by `formation_id`. The dissolve runs one final tick to ensure the latest accumulated state is captured.

**Warm start.** When a formation is *deployed* against an existing `formation_id`, `lifecycle::spawn_formation` hydrates the mental model (and the persisted momentum row and rally tokens) before the first tick. The formation begins at Cold momentum but with the prior model populated.

**Boot restore.** `spawn_formation` runs off the `FormationCommand::Deploy` arm, and at boot `springtaled`'s `runtime::boot::formations::restore_formations` sends exactly that command for every stored formation that was `active` or `paused` at shutdown (followed by `Pause` for the paused ones). So a restart brings formations back on its own, each warm-starting from its persisted momentum row, rally tokens and mental model.

**Inspection.** `GET /formations/{id}` includes a summary of the mental model in the response. Full schema lives in `crates/springtale-cooperation/src/mental_model/types.rs`.

## References

- [1] Full specification: [`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md)
- [2] Wiring into the bot runtime: [architecture.md §6](architecture.md)
- [3] Colony canvas visual reference: [colony-canvas.md](colony-canvas.md)
- [4] Mental model schema: `crates/springtale-store/src/schema/sql/cooperation.sql`
- [5] Cooperation lib re-exports: `crates/springtale-cooperation/src/lib.rs`
