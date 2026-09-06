# Springtale — Architecture (As-Built)

> **Status:** Reflects working tree · **Updated:** 2026-05-11 · **Source of truth:** the code
>
> This document describes what **actually exists** in the repository today.
> For design intent (threat-model philosophy, Veilid transport,
> accessibility roadmap), see `docs/current-arch/ARCHITECTURE.md` — that
> document is the locked design reference. When this file and `current-arch/`
> disagree, **this file is reality and `current-arch/` is intent**.

---

## Contents

1. [Workspace Layout](#1-workspace-layout)
2. [Dependency Graph](#2-dependency-graph)
3. [Boot Sequence](#3-boot-sequence)
4. [Foundational Crates](#4-foundational-crates)
5. [Connector Framework](#5-connector-framework)
6. [Bot Runtime & Cooperation](#6-bot-runtime--cooperation)
7. [AI Adapters](#7-ai-adapters)
8. [MCP Bridge](#8-mcp-bridge)
9. [HTTP API Surface](#9-http-api-surface)
10. [CLI Surface](#10-cli-surface)
11. [Storage Schema](#11-storage-schema)
12. [Frontend](#12-frontend)
13. [Data Flow](#13-data-flow)

Companion: [`SECURITY.md`](SECURITY.md), [`AUDIT-NOTES.md`](AUDIT-NOTES.md).

---

## 1. Workspace Layout

```
Springtale/
├── crates/                        # Pure Rust library crates
│   ├── springtale-core/           #   rules, pipelines, router, transforms, canvas types
│   ├── springtale-crypto/         #   vault (KDF, AEAD, duress), signatures
│   ├── springtale-transport/      #   Transport trait + Local/Http/Veilid impls
│   ├── springtale-store/          #   SQLite backend + declarative schema (`user_version` v1)
│   ├── springtale-scheduler/      #   cron, fs-watch, job queue, heartbeat, backoff
│   ├── springtale-connector/      #   native + WASM connector framework, capability checker,
│   │                              #   subscription lifecycle
│   ├── springtale-ai/              #   AiAdapter + Anthropic/Ollama/OpenAI-compat/Noop + tool-calling
│   ├── springtale-mcp/             #   rmcp 1.x bridge (stdio), split handler modules
│   ├── springtale-sentinel/        #   behavioural monitor, toxic-pair detection
│   ├── springtale-cooperation/     #   cooperation framework (42 pub modules, zero internal deps)
│   ├── springtale-runtime/         #   shared init, dispatch, operations, approval gate,
│   │                              #   token quota, trigger lifecycle, extraction, embedded
│   │                              #   runtime, LiveFormationReader
│   ├── springtale-bot/             #   runtime, router, conversation engine, colony commander,
│   │                              #   cooperation glue, orchestrator, handler, identity,
│   │                              #   memory, tool_runner, 25-step formation tick
│   ├── springtale-wit/             #   WIT world for WASM Component Model embedding (G3)
│   ├── springtale-py/              #   pyo3 Python bindings — cdylib + rlib (G3)
│   └── libsqlite3-sys-mc/          #   vendored sqlite shim (SQLite3MultipleCiphers)
│
├── connectors/                    # First-party connectors (all native Rust today)
│   ├── connector-bluesky   connector-browser   connector-discord
│   ├── connector-filesystem connector-github   connector-http
│   ├── connector-irc       connector-kick      connector-matrix (deferred)
│   ├── connector-nostr     connector-opencode  connector-presearch
│   ├── connector-shell     connector-signal    connector-slack
│   ├── connector-telegram
│
├── apps/
│   ├── springtaled/               # Daemon — axum HTTP API, boot, scheduler wiring
│   └── springtale-cli/            # clap-based CLI
│
├── tauri/                         # Excluded from workspace (own dep tree)
│   ├── packages/types/            #   shared TS types
│   ├── packages/ui/               #   shared SolidJS components + DataProvider interface
│   ├── apps/desktop/              #   Tauri 2 desktop shell
│   └── apps/dashboard/            #   SPA served by springtaled
│
├── sdk/connector-sdk/             # Excluded — wasm32-unknown-unknown target
└── docs/
    ├── current-arch/              # Locked design intent (do not edit)
    ├── intended-arch/              # Includes COOPERATION.md spec
    └── arch/                      # ← this folder (as-built)
```

*Fig. 1. Workspace tree.*

Workspace members are declared in `Cargo.toml:1-40`. `matrix-sdk` is
held due to CVE-2025-70873 in its pinned rusqlite 0.37; Springtale uses
rusqlite 0.39.

---

## 2. Dependency Graph

```
                    springtale-core     springtale-cooperation
                   /       |       \           (zero-dep peer)
                  /        |        \                 │
    springtale-store  springtale-crypto               │
          |                 |                         │
          |                 +-----------+             │
          |                 |           |             │
  springtale-scheduler  springtale-transport          │
          |                 |                         │
          +-------+---------+                         │
                  |                                   │
        springtale-connector ──── springtale-sentinel │
                  |                                   │
        springtale-ai        springtale-mcp           │
                  \          /                        │
                   \        /                         │
                springtale-runtime ───────────────────┤
                        |                             │
                 springtale-bot  ──────────────────────┘
                        |
                   springtaled
                        |
                 springtale-cli
```

*Fig. 2. Crate dependency graph. `springtale-cooperation` is a zero-dep
peer consumed by runtime, bot, and (for schema) store.*

Dependencies flow downward only. No circular edges. Rule enforced by
`.claude/rules/backend/crate-structure.md`.

---

## 3. Boot Sequence

`springtaled` boot is a 9-step ordered pipeline split between the daemon
and the shared runtime crate.

```
main.rs (apps/springtaled/src/main.rs)
  │
  ├─[1] Install rustls::crypto::ring provider             main.rs:13
  ├─[2] tracing_subscriber init (EnvFilter)               main.rs:16
  ├─[3] config::load_config() → LoadedConfig              main.rs:23
  └─[4] runtime::boot(config, connector_configs)          main.rs:32
        │
        │  apps/springtaled/src/runtime/boot/mod.rs:20-167
        │
        ├─[1] Log + 0.0.0.0 bind warning                  boot/mod.rs:25
        ├─[2] init_crypto()                               boot/crypto.rs:8
        │       vault OR ephemeral, keypair,
        │       api_token_hash, db_key_hex
        ├─[3] springtale_runtime::init(&RuntimeConfig)    init.rs:28-78
        │       store, RuleEngine, WasmEngine (+ epoch
        │       ticker), ConnectorRegistry (inventory),
        │       AiAdapter (ArcSwap), Sentinel, Canvas bus
        ├─[4] init_transport(keypair)                     boot/transport.rs
        ├─[5] init_schedulers()                           boot/schedulers.rs
        │       CronExecutor, FsWatcher, HeartbeatMonitor
        ├─[6] init_job_queue()                            boot/queue.rs
        ├─[7] init_bot(wiring, msg channels, cooperation) boot/bot.rs
        │       bot handle + per-connector shutdowns
        │       (telegram, nostr, irc, discord, slack, signal)
        │       FormationCommand channel installed; runtime
        │       receives a LiveFormationReader impl backed by
        │       the bot's formation set
        ├─[7c] data retention purge task (if configured)  boot/mod.rs
        │       spawned when [store] retention_days is set
        ├─[8] api::build_router(state) + bind             boot/mod.rs
        └─[9] mark `ready=true`, spawn API server
                + event_loop(trigger_rx, engine)          boot/mod.rs:138
                wait on shutdown_signal()
```

*Fig. 3. 9-step boot pipeline.*

**Design notes**

- `boot/crypto.rs` (daemon-only) was split off from the deleted
  `springtale-runtime/src/boot.rs`. Shared init now lives in
  `springtale-runtime/src/init.rs`; daemon-specific crypto and bot wiring
  stays with `springtaled`.
- Passphrase acquisition (`boot/crypto.rs:65-99`) has a 3-way fallback:
  `SPRINGTALE_PASSPHRASE_FILE` (Docker secrets), `SPRINGTALE_PASSPHRASE`
  (dev only), or interactive TTY prompt. Fatal if none available.
- The API token is `HMAC-SHA256(passphrase, "springtale-api-token")`.
  There is no separate API key; rotating the token means rotating the
  vault passphrase.
- AI adapter is hot-swappable at runtime via `ArcSwap<Arc<dyn AiAdapter>>`
  (`state.rs:27`). Config changes to `/config/ai` atomically replace the
  active adapter with zero locking on the read path.

---

## 4. Foundational Crates

### 4.1 `springtale-core`

```
core/src/
├── lib.rs                  # module declarations only
├── canvas/types.rs         # CanvasState, CanvasBlock, CanvasUpdate
├── pipeline/
│   ├── stage.rs            # Stage trait (async call → PipelineContext)
│   ├── context.rs          # trace_id, input, output, errors, fuel_remaining
│   ├── compose.rs          # compose_pipeline()
│   └── error.rs
├── router/dispatch.rs      # dispatch_event(engine, event) → Vec<RuleMatch>
├── rule/
│   ├── engine.rs           # RuleEngine (regex pre-compile at add_rule)
│   ├── action.rs           # Action enum
│   ├── trigger.rs          # Trigger enum
│   ├── condition.rs        # Condition enum
│   ├── evaluate.rs         # evaluate_condition()
│   ├── template.rs         # ${trigger.field} interpolation
│   ├── parse.rs            # TOML + NL parsing
│   └── types.rs            # Rule, RuleId, RuleStatus, RuleVersion
└── transform/
    ├── extract.rs          # field extraction
    ├── filter.rs           # data filtering
    └── format.rs           # resolve_template()
```

**Key types**

| Type | File:Line | Notes |
|---|---|---|
| `Stage` trait | `pipeline/stage.rs:12` | `async fn call(PipelineContext) -> Result<PipelineContext, PipelineError>` |
| `RuleEngine` | `rule/engine.rs:54` | Pre-compiles regex at `add_rule()`; evaluation is pure |
| `Action` | `rule/action.rs:21` | RunConnector, SendMessage, WriteFile, RunShell, Notify, Chain, Transform |
| `Trigger` | `rule/trigger.rs:7` | Cron, FileWatch, Webhook, ConnectorEvent, SystemEvent |
| `CanvasState` | `canvas/types.rs:57` | Broadcast to SolidJS via SSE |

### 4.2 `springtale-store`

SQLite-only backend behind a `StorageBackend` trait. `SqliteBackend`
uses `rusqlite 0.39` via SQLite3MultipleCiphers (ChaCha20-Poly1305
encryption at rest, WAL mode, `0o600` perms).

```
store/src/backend/
├── trait_.rs               # StorageBackend trait
├── sqlite/
│   ├── mod.rs              # SqliteBackend
│   ├── cooperation.rs      # momentum / rally / mental_model queries
│   └── {rules, connectors, events, jobs, sessions, memory,
│        aliases, audit, safety, formations, execution, wasm}.rs
├── memory/                 # in-memory impl (tests)
│   └── cooperation.rs      # in-memory cooperation tables
└── wipe.rs                 # panic wipe hooks
```

See [§11 Storage Schema](#11-storage-schema) for tables.

### 4.3 `springtale-scheduler`

```
scheduler/src/
├── cron/executor.rs        # CronExecutor (tokio-cron-scheduler)
├── watcher/fs_watcher.rs   # FsWatcher (notify-rs)
├── heartbeat/monitor.rs    # HeartbeatMonitor (liveness + TTL)
├── queue/
│   ├── producer.rs         # JobProducer, Job, JobStatus
│   └── consumer.rs         # JobConsumer (dequeue + retry)
└── retry/backoff.rs        # BackoffConfig (exponential)
```

**Job queue is in-memory today.** `JobProducer` is an mpsc sender; the
schema for SQLite-backed jobs exists in `schema/sql/jobs.sql` but
`StorageBackend::enqueue_job()` is not yet wired. Producer comments note
the API is stable and only the backing changes. See AUDIT-NOTES §1.

### 4.4 `springtale-transport`

```
transport/src/
├── transport/trait_.rs     # Transport trait (send/recv/node_id/name)
├── local/unix_socket.rs    # LocalTransport — present, Unix socket
├── http/server.rs          # HttpTransport — present, rustls mTLS
├── crypto_provider.rs      # installs rustls-post-quantum as process default
├── safe_http.rs            # typed reqwest wrapper (rustls-only, no raw Client::new)
└── veilid/stub.rs          # VeilidTransport — stub, returns NotConnected
```

Wire envelope is length-delimited JSON (`WireMessage { sender: [u8;32],
message: Message }`). `Message::MAX_MESSAGE_SIZE = 16 MiB`. `recv()` is
cancel-safe for `tokio::select!`. `VeilidTransport` is a stub: every
method returns `TransportError::NotConnected`. The struct has a private
constructor and is currently a compile-time placeholder only.

---

## 5. Connector Framework

### 5.1 Trait

`crates/springtale-connector/src/connector/trait_.rs:27-82`

```rust
#[async_trait]
pub trait Connector: Send + Sync {
    fn triggers(&self) -> &[TriggerDecl];
    fn actions(&self) -> &[ActionDecl];
    async fn execute(&self, action: &str, input: Value) -> Result<ActionResult>;
    async fn on_event(&self, trigger: &str, handler: Box<dyn EventHandler>);
    fn manifest(&self) -> &ConnectorManifest;
    async fn verify_webhook(&self, headers: &Headers, body: &[u8]) -> Result<()>;
    // default: reject all
}
```

**Invariant** (line 31): the capability layer wraps this trait; every
`execute()` is preceded by `check_action_capabilities()` in the dispatch
path. Connectors **cannot** skip it.

### 5.2 Capability enforcement

```
Manifest declares           User policy              Runtime check
─────────────────           ───────────              ─────────────
[[capabilities]]            CapabilityPolicy:        On each execute():
  type = NetworkOutbound    • AllowAll               1. Try per-action
  host = "api.kick.com"     • DenyAll                   inference:
                            • AllowList(set)            input["host"] → NetworkOutbound
[[capabilities]]            • Interactive (default)     input["path"] → Fs{Read,Write}
  type = ShellExec            (ShellExec always           input["command"] → ShellExec
                               holds pending           2. Fallback: check ALL declared
                               user approval)           3. Error → dispatch aborts
```

*Fig. 4. Capability declaration → verification → runtime check.*

File refs: `manifest/types.rs:10-70`, `manifest/verify.rs:11-71`,
`capability/grant.rs:60-143`, `native/capability.rs:20-42`.

Wildcard `NetworkOutbound` hosts are rejected at manifest validation
(`verify.rs:56-61`). Signatures are Ed25519 over canonical JSON (all
manifest fields except `signature`).

### 5.3 Host trait + two execution models

```
           Arc<dyn ConnectorHost>
         ┌─────────┴──────────┐
         │                    │
NativeConnectorHost     WasmConnectorHost
 (in-process)           (Wasmtime sandbox)
  no sandbox             64 MB mem, 10 M fuel,
  trait object           epoch timeout,
  #[forbid(unsafe)]      per-invocation Store
```

*Fig. 5. Two execution models behind one `ConnectorHost` trait.*

**Native path** (`native/runtime.rs:16-97`): wraps `Box<dyn Connector>`,
runs `execute_checked()` which calls `check_action_capabilities()` then
delegates. All 15 first-party connectors ride this path.

**WASM path** (`wasm/connector.rs:45-301`): per-invocation `Store` with
fresh fuel budget and epoch deadline. ABI: guest exports
`execute(action_ptr, action_len, input_ptr, input_len) -> i32` with
length-prefixed JSON in linear memory at offset 1024. Host functions
(currently only `http_request`) gate through `CapabilityChecker::check()`
in `wasm/host_api.rs`. SHA-256 of the wasm bytes is verified against
`manifest.wasm_hash` before module load (`wasm/connector.rs:70`).

Engine config (`wasm/runtime.rs:23-56`):

| Setting | Value |
|---|---|
| `consume_fuel` | true |
| `epoch_interruption` | true |
| `cranelift_opt_level` | Speed |
| memory limit | `limits.memory_bytes` (64 MB default) |
| instance cap | 10 |
| table cap | 10 |
| memory cap | 2 |

**No WASM connector exists today.** All 15 first-party connectors ride
the native path. The Wasmtime host, capability gate, SHA-256 integrity
check, SDK, and per-invocation limits are all built and tested — but
no first-party or community connector actually rides the sandbox. The
SDK under `sdk/connector-sdk/` is usable for authors targeting
`wasm32-unknown-unknown`.

### 5.4 Registry

`registry/store.rs:25-137`. In-memory `HashMap<String, ConnectorEntry>`.
Persistence lives at the application layer via `springtale-store`
(`connectors` table, `schema/sql/connectors.sql`). `get_for_execute()` (line 96-112)
returns a cloned `Arc<dyn ConnectorHost>` + cloned capability checker so
network calls don't hold the registry `RwLock`.

### 5.5 First-party connector inventory

All 15 are native Rust. Matrix is workspace-excluded (deferred).

| Name | Transport | Notable triggers | Notable actions |
|---|---|---|---|
| bluesky | ATProto + Jetstream | note_received, repost, like | create_post, reply, like, repost |
| browser | Chromium (WASM) | dom_element_found, nav_complete | click, fill, navigate, screenshot |
| discord | twilight gateway | interaction_received | send_message, send_embed |
| filesystem | inotify | file_{created,modified,deleted} | read_file, write_file, list_dir |
| github | REST v3 + webhooks | push, pr_opened, issue_opened | create_issue, post_comment, create_branch, commit_file, create_pr |
| http | reqwest | — | GET, POST |
| irc | native IRC | channel_message, user_joined | send_message, join, part |
| kick | OAuth 2.1 + REST | stream_live, chat_message | send_message, start_raid, ban |
| nostr | NIP-44 relays | note_received, dm_received | send_note, send_dm |
| opencode | HTTP to local `opencode serve` | — (action-only) | run_task, continue_session |
| presearch | REST | query_results_received | search, get_trending |
| shell | OS exec | command_exit_received | execute_command/script |
| signal | signal-cli bridge | message_received | send_message, group_invite |
| slack | Socket Mode + webhooks | slash_command, app_mention | send_message, send_blocks |
| telegram | Bot API poll/webhook | message_received, callback_query | send_message, send_photo |

---

## 6. Bot Runtime & Cooperation

The cooperation architecture has two parts:

- `crates/springtale-cooperation/` — the crate with 42 pub modules, zero
  internal Springtale deps. Types, traits, and algorithms.
- `crates/springtale-bot/src/cooperation/` — the glue. Holds the live
  `Formation` struct (mutable runtime fields like `active_task`,
  `fuel`, `liveness`) and the blackboard.

### 6.1 Bot event loop

`crates/springtale-bot/src/runtime/event_loop.rs`. Four-way
`tokio::select!`:

```
           ┌─────────────────────────────────────┐
           │   Bot::run_event_loop()             │
           └──────────────┬──────────────────────┘
                          │
         ┌────────────────┼────────────────┬──────────────────┐
         │                │                │                  │
   connector_rx      rule_rx          cadence_rx        formation_cmd_rx
   (chat messages)   (triggers)       (Tick broadcast)  (FormationCommand)
         │                │                │                  │
   handle_incoming   handle_trigger   handle_cadence_tick  handle_formation_
         │                │                │                  │  command
   router dispatch   engine evaluate   25-step tick        deploy/pause/
         │                │              pipeline          resume/dissolve/
         └──────┬─────────┴─────────────┬──┘                 rally/intent/
                ▼                       ▼                    members
          registry.execute()      per-formation work
```

*Fig. 6. Bot four-way event-loop select.*

Design constraint at `event_loop.rs`: never `?` individual message
processing — a single bad message must never crash the bot. The
`formation_cmd_rx` branch is the **only** code path that materialises
live `Formation` structs from DB rows.

### 6.2 The cooperation crate

```
springtale-cooperation/src/         ── zero internal Springtale deps
├── lib.rs                          re-exports the public API
├── cadence.rs                      §5  Tick bus, AgentId, TickReport
├── momentum.rs / momentum/         §7  Cold/Warming/Hot/Fever gate
├── awareness/                      §8  LocalAwareness, gossip substrate
│   ├── store/                         (InMemory + chitchat)
│   └── swim/                          (SWIM liveness)
├── attention/                      §9  AttentionEconomy (zero-sum)
├── state/                          §10 Workspace + SharedEnvironment
├── consensus.rs                    §11 Ballot, weighted voting
├── commit.rs                       §12 CommitPhase barrier
├── interference/                   §13 4 kinds of conflict detection
├── transformation/                 §14 Role change on capability loss
├── rally/                          §15 Self-healing + supervise drain
├── capability/                     §16 DynamicCapabilitySet (4 layers)
├── recovery/                       §18 DistressSignal → helper
├── comms/                          §19 Bus, dispatcher, state broadcast
├── handoff/                        §20 Direct / flex-chain / sequential
├── mental_model/                   §21 Learned domain knowledge
├── pacing/                         §22 Work/rest (GCRA, L4D Director)
├── sacrifice/                      §24 Deliberate self-cost
├── contract_net/                   CNP (CFP / bid / award)
├── routing/                        L1/L3 task routing
├── stigmergy/                      L0 ambient surfaces
├── supervision/                    Erlang OTP + K8s probes
├── role/                           DynamicRoleTrait
├── authority/                      momentum × layer perm matrix
├── agent/                          per-agent 5-step loop
├── action.rs, action_state.rs      SubTask + ActiveTask state
├── tick_processor.rs               per-tick aggregation
├── dissemination/                  L2 state dissemination
├── peer.rs                         PeerMsg protocol
├── replan/                         CBBA global re-plan
├── utility/                        utility AI scoring
├── layer.rs                        7-layer routing abstraction
├── context.rs                      FormationContext (read-only share)
├── command.rs                      FormationCommand (runtime → bot)
├── types.rs                        FormationId, AgentHealth, etc.
└── error/                          typed errors per concern
```

### 6.3 The 25-step tick pipeline

`springtale-bot::runtime::event_loop::handle_cadence_tick` takes the locks
and loops; the pipeline itself is
`springtale-bot::runtime::tick_steps::run_tick`, one named module per step:

```
 gate. pacing divider — skip this bus tick unless the phase admits it
 1.  build_reports          per-member agent loop (sense → inbox → react → scan)
 2.  momentum.check_decay   inactivity
 3.  update_momentum        success / interference / failure
 4.  liveness               Alive / Suspect / Down
 5.  supervision            supervisor.check_member → SupervisionAction, rally events
 6.  fuel                   per-member fuel consumption
 7.  implicit_signals       publish ImplicitSignal (Overcooked — peers watch bus)
 8.  state_broadcast        broadcast_state on health threshold (L4D "I'm hurt")
 9.  persist_momentum       momentum row → SQLite
 10. publish_context        FormationContext watchers
 11. gossip_awareness       gossip publish + snapshot (Warming+)
 12. log_interference       interference events
 13. check_pacing           phase transition from the tick's StressSample
 14. check_cascade          detect_cascade + attempt_self_rally
 15. check_interventions    L6 commander override
 16. recovery               evaluate_recovery per distress signal
 17. transformation         role transformation for failing members
 18. replan_cbba            global task reallocation
 19. resolve_consensus      vote deadlines
 20. tick_commits           advance commit barriers
 21. expire_commits         completed / timed-out barriers
 22. update_mental_model    mental_model::learning::update_model
 23. orchestrate_step       Fever tier + can_orchestrate()
 24. publish_formation_view cross-formation gossip bus
 25. emit_canvas_update     only when a canvas sender is wired
```

`respond_cfp` is not a step in the agent loop — it fires reactively when a
call for proposals arrives. After the loop over formations,
`handle_cadence_tick` runs the tail passes (`tail::reclaim_dead`,
`drain_member_subs`, `drain_rally_events`, `retain_viable`).

Step 23 decomposes intent into sub-tasks, posts them to the blackboard
under `task:*` keys; members pull via step 1's `scan` phase. Then
`remove_dead_members()` reclaims slots and `formations.retain(is_viable)`
prunes exhausted formations.

### 6.4 Momentum capability gate

The core differentiator: momentum tiers **gate runtime capabilities**.

| Tier | Successes | env read | neighbours | env write | commit | consensus | AI | recruit |
|---|---|---|---|---|---|---|---|---|
| Cold | 0 | ✓ | — | — | — | — | — | — |
| Warming | ≥3 | ✓ | ✓ | — | — | — | — | — |
| Hot | ≥8, 0 interference | ✓ | ✓ | ✓ | ✓ | — | — | — |
| Fever | ≥15, 0 interference | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

*Fig. 7. Momentum tiers gate runtime capabilities.*

Source: `springtale-cooperation/src/momentum.rs`. Gates are enforced via
methods like `can_write_environment()`, `can_use_ai()`,
`can_consensus()` — called before the corresponding action. Cold
formations physically cannot reach the AI adapter because the dispatch
predicate fails.

### 6.5 Formation struct (bot-side)

```rust
pub struct Formation {                           // bot/cooperation/formation.rs
    pub id: FormationId,
    pub members: Vec<FormationMember>,           // peers, no hierarchy
    pub intent: IntentPattern,
    pub momentum: MomentumState,
    pub blackboard: CooperativeBlackboard,
    pub shared_env: SharedEnvironment,
    pub rally: FormationRally,
    pub attention_broker: AttentionBroker,
    pub gossip_store: Arc<dyn GossipStore>,      // InMemory or Chitchat
    pub mental_model: SharedMentalModel,
    pub consensus: ConsensusEngine,
    pub pacing: PacingManager,
    pub supervisor: FormationSupervisor,
    pub bus: CommsBus,
    pub fuel: FuelBudget,
    pub paused: bool,
    // orchestrator is looked up from bot.registry at tick time
}
```

### 6.6 LiveFormationReader

`springtale-runtime::LiveFormationReader` is how the Tauri IPC commands
and the dashboard HTTP API read enriched formation state (momentum,
rally tokens, attention load, guard status, member health, liveness)
from the running bot. The bot installs an impl backed by its formation
set during step 7 of boot; the runtime holds the reader through an
`Arc<dyn LiveFormationReader>` so UI code paths never touch the bot's
internal types.

### 6.7 Cooperation persistence

Two domain SQL files back the cooperation framework:

- `schema/sql/formations.sql` — `formations`, `formation_members`,
  `formation_momentum`, `formation_rally`
- `schema/sql/cooperation.sql` — `coop_writes`, `coop_deposits`,
  `mental_model_{domain, capability, pattern, vocabulary, convention}`

Momentum and rally are persisted every tick. Mental model is persisted
on formation dissolve so later formations with the same id benefit from
what prior instances learned.

### 6.8 Recipes (W-series)

A separate subsystem under `crates/springtale-runtime/src/operations/recipes/`
that wraps the cooperation/connector/rule/AI primitives in click-and-play
"recipes" — blueprints the UI deploys without TOML editing.

```
recipes/
├── types.rs       Recipe, RecipeCategory, FieldVisibility, FieldKind,
│                  RecipeSource (Builtin / User / Community), RecipeBlueprint
├── builtin.rs     Built-in recipe catalogue — shipped in the binary
├── builtin/       One file per recipe
├── library.rs     Server-side list / filter / sort. Built-ins + user merge
├── apply.rs       apply_recipe() — substitute ${input_id} placeholders,
│                  call existing connector/rule/AI ops, return ApplyReport
├── authoring.rs   W2.B field-classification helpers (UI-side authoring)
├── pieces.rs      Modular recipe pieces (sub-blueprints composed at apply)
└── mod.rs
```

**Architecture invariant — backend owns the decisions.** Every "is this
required vs optional vs advanced vs baked" decision lives in the
`Recipe` value the backend returns. Frontends render what they're told;
they never invent categories or classify fields. Same shape feeds
desktop IPC, dashboard HTTP, and any future surface. See
`feedback_thin_frontend_modular_backend` and
`feedback_zero_frontend_logic`.

**Wire:** 16 endpoints under `/recipes/*` (see §9 — Recipes group).

**Storage status:**
- Built-in recipes compile into the binary; no table needed.
- `recipes_user` SQLite table is **planned, not yet shipped** —
  `library::load_user_recipes()` returns `Ok(Vec::new())` until the
  schema lands. User-saved recipes (W2.B) are wire-shaped via the
  `/recipes/user`, `/recipes/user/{id}`, `/recipes/import` endpoints
  but return `OperationError::NotSupported` until storage ships.
- Community recipes (`RecipeSource::Community { author, signature }`)
  are wire-shape only; sentinel signature verification + trusted-author
  flow (W3.A) lights up when the marketplace lands.

**W-series milestones in flight:**

| Milestone | Concern | Status |
|---|---|---|
| W1.C | Progressive-disclosure deploy form (Required → Optional → Advanced) | Shipped |
| W1.D | Preflight + Deploy-button gating | Shipped |
| W1.F | Approval-gate UX dispatcher (Tauri side) | Shipped — see §10.2 below |
| W2.B | User-recipe authoring + storage | Partial (UI shipped, table pending) |
| W3.A | Community signature verification + trust badges | Wire-shape only |

User-facing: [`docs/guide/recipes.md`](../guide/recipes.md). Format:
[`docs/reference/recipes-format.md`](../reference/recipes-format.md).

### 6.9 Executions log + drift detection (Phase B)

Per-chain-fire observability that the legacy tables couldn't answer.
`crates/springtale-runtime/src/operations/executions/`:

- `recorder.rs` — `ExecutionRecorder` trait + `StoreRecorder` + `NoopRecorder`. Dispatcher calls `record_start()` / `record_step()` / `record_finish()`.
- `query.rs` — list / steps / vacuum.
- `drift.rs` — `recipe_drift()` / `rule_drift()` → `DriftReport { latency, rate, classification }`. `DriftClass: Stable | Improving | Degrading | Volatile`.

Persistence: two new tables in `schema/sql/executions.sql`:
- `executions` — ULID PK, per-fire row with `agent_id`, `formation_id`, `momentum_tier`, `mode` (Normal / DryRun), `status` (running / success / error / empty / suppressed), `error_kind` (enum tag), `summary_bytes`.
- `execution_steps` — per-step row with `input_bytes`, `output_bytes`, `error_kind`.

**Privacy posture:** sizes-only by default. No payload content. `error_kind` is an enum tag. Default 14-day retention swept hourly. Content retention is Phase C, opt-in, separate.

**Cooperation envelope:** `springtale-cooperation::execution::ExecutionContext` carries `execution_id` (ULID), `agent_id`, `formation_id`, `momentum_tier`, `rule_id`, `mode`. Threaded through every dispatch so the executions log scopes per (formation, agent, tier).

User-facing: [`docs/guide/executions-and-drift.md`](../guide/executions-and-drift.md).

### 6.10 External workspaces (D1)

The formation's mental-model extension that stores discovered chat
destinations. Per-formation, gossip-replicated within a formation.

- Schema: `schema/sql/mental_model_workspaces.sql`. Row keyed `(formation_id, workspace_key)`. Stores `display_name`, `kind`, `metadata_json`, `first_seen_at`, `last_seen_at`. **No message bodies, no member rosters** — names only.
- Workspace key URIs: `telegram://chat/12345`, `discord://guild/G/channel/C`, etc. Parsed in `crates/springtale-connector/src/workspace_key.rs`. Each connector owns its scheme.
- Harvester: `springtale-runtime::operations::workspaces::harvester::harvest_event` runs on every dispatched event, calls the connector's `MentionExtractor::extract_destinations()`, upserts into the table.
- `MentionExtractor` trait lives in `crates/springtale-connector/src/mention.rs`. Each messaging connector implements it. Pure function — no async, no I/O.

Within-formation gossip replicates new rows across members via the
chitchat substrate. Conflict resolution in
`crates/springtale-cooperation/src/mental_model/external_workspaces.rs::merge_gossip_delta`.

User-facing: [`docs/guide/external-workspaces.md`](../guide/external-workspaces.md).

### 6.11 Dedupe + Extract (Phase A)

Two `Action` variants in `springtale-core::rule::action` that
together make polling recipes practical.

- `Action::Extract { source, kind }` — parse bytes into structured data. `ExtractKind` variants: `Readability` (article body), `Css { schema }` (selectors), `JsonPath { schema }` (RFC 9535), `Feed` (RSS/Atom/JSON Feed), `Ical { window_days }`, `LlmSchema { schema }` (structured AI output), `Passthrough`.
- `Action::Dedupe { key, bucket, history }` — short-circuit the chain when the key has been seen. Plaintext key never persisted; blake3 hex digest stored in `dedupe_seen` table. Bounded to `history` entries per bucket (default 10,000) with LRU prune.

Scoping per Phase 0.4 cooperation alignment: `dedupe_seen` is keyed by `(formation_id, rule_id, bucket, key_hash)`. `formation_id NULL` = global rule; `NOT NULL` = formation-scoped. Two formations running the same rule see independent dedupe state. Schema: `schema/sql/dedupe.sql`.

Hit path: chain returns `ChainError::Suppressed`, execution row gets `status = "empty"` (not failed). Drift detector distinguishes empty from error.

User-facing: [`docs/guide/dedupe-and-extract.md`](../guide/dedupe-and-extract.md).

### 6.12 Recipe authoring tools

Authoring-time helpers under `crates/springtale-runtime/src/operations/`:

- `preflight/` (W1.D) — checks the recipe's preconditions before Deploy. `PreflightReport { items, deployable }`. Checks include required inputs, input format, connector loaded/capable, AI config, structured outputs support, host allow-list, cron sanity.
- `preview.rs` (W2.C) — fresh in-memory `RuleEngine`, fires synthetic trigger, returns `PreviewReport { steps }`. No side effects.
- `test_step.rs` (W2.C / Phase C) — runs chain in `ExecutionMode::DryRun` up to a single target step, returns recorded `StepOutput`. Read-only arms run for real; side-effecting arms stubbed. Persisted to executions log with `mode = "dry_run"`.

UI: `PreflightChecklist`, `PreviewPanel`, `TestStepButton`,
`SelectorPickerOverlay`. The selector picker opens a Tauri webview at
the recipe's target URL, injects `picker.js`, returns the chosen CSS
selector (authoring-time only, not a headless-browser feature).

User-facing: [`docs/guide/recipe-authoring-tools.md`](../guide/recipe-authoring-tools.md).

---

## 7. AI Adapters

`crates/springtale-ai/src/`.

```
AiAdapter trait (adapter/trait_.rs)
    │
    ├── complete(AiRequest)            → Result<String, AiError>
    ├── complete_with_tools(...)       → tool-calling loop entry (ToolCall / ToolResult)
    ├── stream(AiRequest)              → Result<AiStream, AiError>  (SSE / NDJSON deltas)
    ├── parse_rule(String)             → Result<Rule, AiError>
    ├── is_available() -> bool
    └── structured_extractor()         → Option<&dyn StructuredExtractor>  (schema-constrained JSON)

AiStream = Pin<Box<dyn futures_core::Stream<Item = Result<StreamChunk, AiError>> + Send>>
```

Adapters are wrapped in `GuardrailAdapter` (`src/guardrail/`) at boot:
wall-clock timeout fence, output size cap, refusal-rate counters, and a
per-bot daily token quota behind the `TokenQuota` trait (SQLite-backed
impl in `springtale-runtime::quota`, persisted in `ai_token_usage`).

| Adapter | Streaming | Status |
|---|---|---|
| `AnthropicAdapter` | ✓ (SSE, Claude Sonnet 4) | Full |
| `OllamaAdapter` | ✓ (NDJSON) | Full |
| `OpenAiCompatAdapter` | ✓ (SSE) | Streaming for text deltas; tool calling via `complete_with_tools()` |
| `NoopAdapter` | — | `AiError::Disabled` for everything |

Factory at `factory.rs:16-38` — selection priority Anthropic → OpenAI →
Ollama → Noop. Hot-swapped at runtime via `ArcSwap<Arc<dyn AiAdapter>>`
on the `RuntimeState` (`state.rs`). `POST /config/ai` replaces the active
adapter atomically.

### Sanitization

Two layers before every request:

1. **Compile-time**: `AiRequest` is a closed enum with concrete `String`
   fields. Secrets can't be passed through the type system.
2. **Runtime**: `Sanitizer` (`sanitize/sanitizer.rs`) scans for PII
   (SSN/CC/phone/email), credentials (`sk-`/`pk-` prefixes, bearer
   tokens), prompt injection patterns from the OWASP LLM Cheat Sheet,
   excessive length (>10k chars), and suspicious base64 blobs. Policy
   modes: `Warn` (default), `Redact`, `Block`.

Called from all three production adapters before sending (e.g.
`OllamaAdapter:34`, `OpenAiCompatAdapter:57`, `AnthropicAdapter:67`).

---

## 8. MCP Bridge

`crates/springtale-mcp/`. Built on `rmcp` 1.x.

```
ConnectorMcpServer (server/builder.rs:34)
  ├── connector: Arc<dyn Connector>
  ├── capability_checker
  └── tools: cached at construction from connector.actions()

list_tools(): if ALL capabilities approved → return tools
              else → return [] (defense-in-depth discovery filter)

call_tool(name, args):
  1. JSON Schema validate args against action.input_schema (jsonschema crate)
  2. check_action_capabilities() (re-check, not just trust list_tools)
  3. connector.execute(name, args)
  4. Return ActionResult → MCP tool result
```

*Fig. 8. MCP bridge. Capabilities are re-checked at both `list_tools` and `call_tool` — MCP does not bypass the sandbox.*

**Transport: stdio only.** `start_stdio_server()` reads JSON-RPC from
stdin, writes to stdout. No HTTP or SSE MCP transport is wired. Exit on
stdin EOF.

---

## 9. HTTP API Surface

Defined in `apps/springtaled/src/api/`. Router built at `api/mod.rs:93`.
All authenticated routes require `Authorization: Bearer <token>`; SSE
streams take a one-time ticket from `POST /stream/ticket` instead (the
token never goes in a URL). Middleware stack: rate limit
(default 100 req/s, `tower::limit::RateLimitLayer`), 1 MiB body cap,
30 s timeout, CSP headers, `X-Frame-Options: DENY`.

**Public routes**

| Method | Path | Handler |
|---|---|---|
| GET | `/health` | `health::health` |
| GET | `/ready` | `health::ready` |
| GET | `/ui`, `/ui/*path` | `dashboard::serve_*` (embedded SPA) |

**Authenticated routes** (grouped). Webhook routes live here — they require the same bearer token as every other authenticated endpoint. Each connector performs its own signature check on the body via `Connector::verify_webhook()`.

| Group | Routes |
|---|---|
| **Connectors** | `GET /connectors`, `/connectors/schemas`, `/connectors/available`; `POST /connectors/setup`, `/connectors/install`; `DELETE /{name}`, `/{name}/cascade`; `GET /{name}/config`, `/{name}/outputs`; `POST /{name}/enable`, `/{name}/disable`, `/{name}/test`, `/{name}/upsert-config`, `/{name}/reload` (G4 hot-reload) |
| **Rules** | `GET|POST /rules`; `POST /rules/parse`; `GET /rules/schema`; `PUT|DELETE /rules/{id}`; `POST /rules/{id}/{toggle|run|reassign}`; `POST /rules/connector`; `GET /rules/connector/{name}` |
| **Formations** | `GET|POST /formations`; `GET /formations/{id}` (via `LiveFormationReader`); `GET /formations/{id}/commands` (3×3 grid); `GET /formations/{id}/members/eligible`; `GET /formations/intents`; `POST /formations/deploy-team`; `POST /formations/{id}/{deploy|pause|resume|dissolve|rally|cycle-intent|cycle-autonomy}`; `PUT /formations/{id}/intent`; `POST|DELETE /formations/{id}/members`; `POST /formations/{id}/toggle-guard` |
| **Cooperation** | `GET /cooperation/events` (SSE — formation lifecycle, momentum, rally, interference events) |
| **Agents** | `GET /agents/states`; `GET|PUT /agents/{name}/autonomy`; `POST /agents/{name}/autonomy/step` |
| **Canvas** | `GET /canvas`, `/canvas/connections`; `GET /canvas/stream` (SSE). Canvas is read-only over HTTP — layout writes go through the Tauri IPC layer, not the daemon API. |
| **Events** | `GET /events`; `GET /events/stream` (SSE) |
| **Config** | `GET|PUT /config/{key}`; `GET /config`; `POST /config/ai`, `/config/ai/configure`, `/config/connector/{name}`; `GET|PUT /config/heartbeat` |
| **Authors** | `GET /authors`; `POST|DELETE /authors/{name}` |
| **Bot admin** | `GET /bot/{status|formations|memory}` |
| **Sessions** | `GET /sessions` |
| **Memory** | `POST /memory/audit`, `/memory/compact` |
| **Safety** | `GET|PUT /safety`; `POST /safety/disguise/active` (G5d); `POST /safety/disguise/profile` (G5f); `POST /safety/panic_tap_count` |
| **Data** | `POST /data/export`. Import is CLI-only (`springtale-cli data import`) — runs offline against the local SQLite backend. |
| **Send** | `POST /send` (capability-gated direct action dispatch) |
| **Diagnostics** | `GET /diagnostics` (Doctor flow) |
| **Fixes** | `GET /fixes`, `GET /fixes/{id}`, `POST /fixes/{id}/apply` |
| **Onboarding** | `GET /onboarding/platforms`; `POST /onboarding/{platform}` |
| **Templates** | `GET /templates`; `POST /templates/{name}` |
| **Recipes** | `GET /recipes`, `/recipes/categories`, `/recipes/{id}`, `/recipes/{id}/pieces`, `/recipes/{id}/export`; `POST /recipes/{id}/{favorite|recent|apply|render|preflight|preview|fork}`; `POST /recipes/user`, `/recipes/import`; `DELETE /recipes/user/{id}` |
| **Webhooks** | `POST /webhook/{connector}/{trigger}` |

CSRF protection middleware (`require_csrf_protection`) sits in front of
all authenticated state-changing methods.

Full auth + middleware + response header details in [`SECURITY.md §7`](SECURITY.md).

---

## 10. CLI Surface

`apps/springtale-cli/src/cli.rs`. Global `--json` flag switches
human-readable tables to JSON.

```
springtale
├── init                                  create ~/.local/share/springtale
├── new <template>                        scaffold from starter template
├── server start                          run daemon inline (dev)
├── doctor                                diagnostic checks (same as /diagnostics)
├── fix <error-id>                        apply auto-repair (same as /fixes/apply)
├── trace [--connector --rule]            real-time execution trace
├── panic                                 emergency wipe (no confirm)
│
├── connector
│   ├── list
│   ├── enable <name>
│   ├── disable <name>
│   ├── remove <name>
│   └── install <path>                    verify signature + manifest
│
├── rule
│   ├── list
│   ├── toggle <id>
│   ├── add <file>
│   ├── run <id>                          dry-run against synthetic event
│   ├── update <id> <file>
│   └── delete <id>
│
├── events [--limit N] [--connector NAME]
│
├── vault duress-setup
├── crypto rotate-vault-key
├── bot
│   ├── pair-init                         generate pairing code
│   └── panic-unpair                      revoke all paired users
│
├── travel
│   ├── prepare --backup-to <path>
│   └── restore  --from      <path>
│
├── memory
│   ├── audit
│   └── compact [--max-entries N]
│
├── data
│   ├── export [--output <path>] [--encrypt]
│   ├── import --input <path>
│   └── purge
│
└── agent set-autonomy <name> <level>
```

Command handlers live in `apps/springtale-cli/src/commands/*`.

---

## 11. Storage Schema

The schema is **declarative**, not migration-driven. One canonical
DDL file per domain lives under
`crates/springtale-store/src/schema/sql/`, applied as a single
transaction at backend open. `PRAGMA user_version` fingerprints the
schema; old dev DBs carrying the pre-launch `_migrations` marker are
auto-wiped and rebuilt.

| Domain file | Tables / purpose |
|---|---|
| `connectors.sql` | `connectors` (installed connectors + manifest JSON) |
| `rules.sql` | `rules` (incl. `activation_error` for /diagnostics) |
| `events.sql` | `events` (timeline metadata) |
| `jobs.sql` | `jobs` (queue schema; in-memory mpsc today) |
| `bot.sql` | `bot_sessions`, `user_prefs`, `bot_memory` (encrypted BLOB), `bot_aliases` |
| `audit.sql` | `audit_trail` (append-only, 3 indices) |
| `safety.sql` | `safety_config` (single row, disguise + panic_tap defaults) |
| `formations.sql` | `formations`, `formation_members`, `formation_momentum`, `formation_rally` |
| `runtime_config.sql` | `config_store` (KV, UI-driven runtime config, seeded defaults) |
| `execution.sql` | `execution_results` (capped at 100/connector; legacy) |
| `executions.sql` | `executions`, `execution_steps` (Phase B per-chain-fire observability) |
| `wasm.sql` | `wasm_binaries` (content-addressed, SHA-256 + Ed25519 sig) |
| `cooperation.sql` | `coop_writes`, `coop_deposits`, `mental_model_{domain, capability, pattern, vocabulary, convention}` |
| `mental_model_workspaces.sql` | `mental_model_workspaces` (D1 discovered chat destinations) |
| `dedupe.sql` | `dedupe_seen` (`Action::Dedupe` blake3 key digests) |
| `approvals.sql` | `pending_approvals`, `tool_loop_checkpoints` (ShellExec approval gate + resumable tool loops) |
| `ai_token_usage.sql` | `ai_token_usage` (per-bot daily token counters) |

Schema-apply ordering and the `SCHEMA_VERSION` constant live in
`schema/apply.rs`; bump the constant when DDL shape changes.

**Notes**

- `bot_memory` is encrypted at rest (`content_encrypted BLOB`, `nonce
  BLOB`) but not compressed.
- `safety_config` is forced single-row via `CHECK (id = 1)`.
- `execution_results` auto-prunes on insert (oldest dropped once a
  connector exceeds 100 rows).
- `jobs` schema is ready for persistent queues; the current `JobProducer`
  still uses in-memory mpsc (see AUDIT-NOTES §1).
- `formation_momentum` is upserted every tick; `formation_rally` is
  upserted on every token consumption.
- `mental_model_*` tables are written on formation dissolve so later
  formations with the same id recover accumulated conventions,
  patterns, and vocabulary.
- The whole DB file is encrypted at rest via SQLite3MultipleCiphers
  (ChaCha20-Poly1305), key derived from the vault passphrase.

---

## 12. Frontend

`tauri/` is workspace-excluded (pnpm + Tauri 2 + SolidJS 1.9 + Tailwind 4
+ Vite 6).

```
tauri/
├── packages/
│   ├── types/              # TS types mirroring Rust schemas + ts-rs generated
│   │                       #   (FormationDelta, FormationOutcome, FormationStatus,
│   │                       #    FormationView under types/src/generated/)
│   └── ui/                 # shared SolidJS components, DataProvider interface
│        └── src/
│            ├── Canvas.tsx                # theme/provider-aware top-level wrapper
│            └── colony/    # ColonyShell, ColonyCanvas, Viewport,
│                           # BottomPanel, TopBar, TeamBuilder,
│                           # ConnectorConfigPanel, AiConfigPanel,
│                           # AppSettingsPanel; geometry + mappers;
│                           # colony.css + sprites.css.
│                           # Plus the G-series + W-series overlays:
│                           #   ApprovalCard (W1.F),
│                           #   EventRibbon (G6 cross-formation),
│                           #   MemberPickerOverlay, ModeSelectOverlay,
│                           #   PreflightChecklist (W1.D),
│                           #   PreviewPanel (W2.C),
│                           #   ProofOfLifePanel, RuleBuilderOverlay,
│                           #   SafetyPanel (disguise + quick-hide + panic-tap),
│                           #   RecipeAuthorPanel, RecipeCard,
│                           #   RecipeDeployPanel, RecipeLibraryOverlay,
│                           #   RecipeQuickView,
│                           #   AiSchemaEditor (Phase B structured AI),
│                           #   CronFrequencyChip, DeploySummaryModal,
│                           #   DriftBadge (Phase B drift trend),
│                           #   ExecutionsPanel (Phase B log viewer),
│                           #   SelectorPickerOverlay (web-recipe authoring),
│                           #   TestStepButton (W2.C single-step DryRun),
│                           #   WorkspaceTargetPicker (D1 external workspaces).
│            └── dashboard/ # context, model, query, types (DataProvider ~60 methods)
│
└── apps/
    ├── desktop/
    │   ├── src/                              # SolidJS UI
    │   └── src-tauri/src/commands/           # 32 command modules:
    │                                         #   agent, approval (W1.F gate dispatcher),
    │                                         #   authors, bot, canvas, config, connectors,
    │                                         #   cooperation (G6 IPC + SSE),
    │                                         #   data, diagnostics, drift (Phase B trend),
    │                                         #   events, executions (Phase B log),
    │                                         #   fixes, formations, heartbeat, memory,
    │                                         #   onboarding, panic, quick_hide (G5g),
    │                                         #   recipes, rules, safety, selector_picker
    │                                         #   (recipe-authoring overlay), send,
    │                                         #   sessions, templates, test_step (W2.C
    │                                         #   single-step DryRun), travel,
    │                                         #   tray (G5f), vault, workspaces (D1);
    │                                         #   plus runtime_guard
    │
    └── dashboard/
        └── src/provider.ts                   # HTTP + SSE DataProvider
```

*Fig. 9. Frontend workspace layout.*

### DataProvider abstraction

```
           DashboardState (SolidJS store)
                      ▲
                      │
              DataProvider (interface)
              ┌───────┴───────┐
              │               │
      DesktopProvider     WebProvider
              │               │
        Tauri invoke()    HTTP + SSE
              │               │
      src-tauri commands  springtaled /api
              │               │
              └───────┬───────┘
                      ▼
         springtale-runtime (LiveFormationReader, operations)
```

*Fig. 10. DataProvider abstraction. The frontend never calls the backend directly — always through the provider.*

- ~60 async methods on `DataProvider` covering connectors, rules,
  events, formations (including rich live state via
  `LiveFormationReader`), config, agents, canvas, memory, authors, data
  export, diagnostics, fixes, onboarding, templates, send.
- Subscribe methods return `() => void` unsubscribe fns.
- **Desktop**: `createDesktopProvider()` wraps `invoke()`; real-time via
  Tauri `listen("event-fired")` and `listen("canvas-update")`.
- **Web**: `createWebProvider()` wraps fetch + `EventSource` on the
  multiplexed `GET /stream?ticket=...` (`event` / `canvas` /
  `cooperation` frames). It fetches a single-use 30-second ticket from
  `POST /stream/ticket` first, because `EventSource` cannot set custom
  headers and the bearer token never goes in a URL.
- Rule from `.claude/rules/frontend/solidjs-conventions.md`: components
  never call `invoke()` directly — always through the provider.

### Colony canvas

The primary UI surface is an RTS-style ecosystem view:

- Connectors → pixel nodes
- Rules / agents → springtails (sprites near their node)
- Formations → zones (dashed ellipses)
- Pipelines → mycelium (SVG paths)

Live state flows through two channels:

- `/canvas/stream` SSE → `CanvasState` delta updates
- `LiveFormationReader` → rich formation state (momentum tier, rally
  tokens, attention load, guard status, aggregate operational / load /
  fuel, member health + liveness)

**Formation command grid** (StarCraft-style 3×3): DEPLOY / PAUSE /
RESUME / RALLY / INTENT / GUARD / ADD MBR / RM MBR / REMOVE. Each
button maps to a `/formations/*` endpoint, which pushes a
`FormationCommand` onto the bot's command channel.

**Formation detail card** shows rally pips (Monster Hunter carts),
attention distribution bar (Army of Two aggro meter), per-member health
+ liveness icons (K8s-style probe states encoded via opacity).

### Themes

Two CSS-only themes:

- Colony (forest) — original, Silkscreen font, soil palette
- Chiral diorama — Death-Stranding-inspired, default in Tauri desktop
  since April 2026

Switching themes changes zero backend behaviour. Theme selection lives
in `AppSettingsPanel.tsx`.

See [`../guide/colony-canvas.md`](../guide/colony-canvas.md) for a
user-facing tour.

---

## 13. Data Flow

### 13.1 Event → action (happy path)

```
  Connector trigger (webhook / poll / fs-event / cron)
            │
            ▼
   TriggerEvent { trigger_type, connector, payload }
            │
            ▼
   RuleEngine::evaluate()  → Vec<RuleMatch>
            │                   (pre-compiled regex, pure fn)
            ▼
   router::dispatch_event()
            │
            ▼
   JobProducer::enqueue(Job { payload, max_attempts })
            │                   (mpsc today; SQLite-backed planned)
            ▼
   JobConsumer::dequeue() → exponential backoff on failure
            │
            ▼
   dispatch_action(action, registry, sentinel)   runtime/dispatch.rs
            │
            ▼
   sentinel.evaluate(action, connector)        → Go | Throttle | Pause | Quarantine
            │                                    (every action, no bypass)
            ▼
   per-action branch
            │   ├─ RunConnector  → registry.execute(name, action, input)
            │   │                  → CapabilityChecker.check()
            │   │                  → NativeConnectorHost / WasmConnectorHost
            │   ├─ WriteFile     → filesystem with path capability
            │   ├─ RunShell      → shell connector (blocking approval)
            │   ├─ SendMessage   → chat connector
            │   ├─ AiComplete    → AiAdapter.complete() (sanitized)
            │   ├─ Transform     → template resolver
            │   ├─ Chain         → recursive dispatch_with_depth (max 15)
            │   └─ Notify        → canvas bus + events table
            │
            ▼
   sentinel.report(action, outcome)              success | failure rows
            ▼
   StorageBackend::log_event()
   StorageBackend::complete_job()
            │
            ▼
   canvas_tx.send(CanvasUpdate)  → SSE → DataProvider → SolidJS signal
```

*Fig. 11. Event → action happy path. Sentinel evaluates every action before the per-action branch.*

### 13.2 Chat message → bot response

```
  Connector chat message (Telegram/Discord/IRC/...)
            │
            ▼
  bot.connector_rx  (mpsc into event_loop)
            │
            ▼
  handle_incoming_message()
            │
            ▼
  Router (router/{prefix,pattern,alias,fallback}.rs)
            │
       ┌────┴────┐
       ▼         ▼
  Prefix hit  No match
  (/search)   (fallback)
       │         │
       ▼         ▼
  Handler    Fallback AI
  dispatch   (Fever-gated; else "unknown command")
       │         │
       ▼         ▼
  ActionResult → response template → connector.execute(send_message)
```

*Fig. 12. Chat message routing in the bot runtime.*

### 13.3 Cadence tick → formation orchestration

```
  CadenceBus::tick() (broadcast, generous window)
            │
            ▼
  bot.cadence_rx  (per-bot receiver)
            │
            ▼
  handle_cadence_tick()
       │
       ├─ for each Formation:
       │     record_success()         → momentum.try_promote()
       │     persist momentum         → config_store[momentum:{id}]
       │     if can_orchestrate():    (Fever + orchestrator present)
       │         AiAdapter.complete(intent_prompt)
       │         parse subtasks → post to CooperativeBlackboard
       │
       └─ members pull subtasks from blackboard (pull, not push)
```

*Fig. 13. Cadence tick → orchestrator → blackboard → members. Orchestrator is Fever-gated; Cold/Warming/Hot formations never reach the AI adapter.*

---

*End of ARCHITECTURE.md. See [`SECURITY.md`](SECURITY.md) for the
security posture audit and [`AUDIT-NOTES.md`](AUDIT-NOTES.md) for known
drift, gaps, and in-flight work.*
