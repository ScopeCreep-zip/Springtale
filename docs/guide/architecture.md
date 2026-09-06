# Architecture

Springtale is a Rust workspace of ~33 crates — 14 library crates (12 core + `springtale-py` for Python bindings + `springtale-wit` for WASM Component Model embedding), 15 first-party connectors, 2 applications, plus a Tauri frontend (excluded from the workspace, built separately). This guide explains how they fit together.

For the as-built architecture with file:line refs, see [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md). For the locked design intent, see [`docs/current-arch/ARCHITECTURE.md`](../current-arch/ARCHITECTURE.md).

## 1. The Workspace

Every crate has a single responsibility. Dependencies flow strictly downward — no cycles, no upward references.

```
                    ┌─────────────────────────────────────────────────┐
                    │                  Applications                   │
                    │  springtaled    springtale-cli    Tauri shell   │
                    └──────┬─────────────────┬────────────────┬───────┘
                           │                 │                │
                           v                 v                │
                    ┌─────────────────────────────────────┐   │
                    │          springtale-bot             │   │
                    │  runtime, router, cooperation       │   │
                    │  glue, orchestrator, memory,        │   │
                    │  handlers, tool_runner              │   │
                    └──────────┬────────────────┬─────────┘   │
                               │                │             │
                               │                v             │
                               │       ┌──────────────────┐   │
                               │       │springtale-       │   │
                               │       │cooperation       │   │
                               │       │ 42 pub modules   │   │
                               │       │(cadence, rally,  │   │
                               │       │ momentum, …)     │   │
                               │       │zero internal deps│   │
                               │       └──────────────────┘   │
                               │                              │
                    ┌──────────▼──────────────────────────┐   │
                    │        springtale-runtime           │◄──┘
                    │  shared init, dispatch, operations  │
                    │  LiveFormationReader                │
                    └──────┬───────────────────────┬──────┘
                           │                       │
         ┌─────────────────┼──────────┬────────────┼──────────┐
         │                 │          │            │          │
         v                 v          v            v          v
    ┌────────┐        ┌────────┐  ┌────────┐  ┌────────────┐  │
    │  mcp   │        │   ai   │  │sentinel│  │ scheduler  │  │
    │ rmcp   │        │ Noop / │  │ toxic  │  │ cron, fs,  │  │
    │ bridge │        │ Ollama │  │ pairs  │  │ jobs, hb   │  │
    │        │        │ OpenAI │  │        │  │            │  │
    │        │        │ Anthro │  │        │  │            │  │
    │        │        │+ tools │  │        │  │            │  │
    └────┬───┘        └───┬────┘  └───┬────┘  └─────┬──────┘  │
         │                │           │             │         │
         └────────────────┴─────┬─────┴─────────────┘         │
                                v                              │
                    ┌──────────────────────────┐               │
                    │    springtale-connector  │               │
                    │  trait, registry,        │               │
                    │  manifest signing,       │               │
                    │  capability system,      │               │
                    │  Wasmtime sandbox,       │               │
                    │  subscription lifecycle  │               │
                    └──────┬──────────┬────────┘               │
                           │          │                        │
                           v          v                        │
                    ┌────────┐   ┌─────────┐                   │
                    │ store  │   │ crypto  │                   │
                    │ SQLite │   │ vault,  │                   │
                    │ + 11   │   │ Ed25519,│                   │
                    │ migrs  │   │ Argon2id│                   │
                    └───┬────┘   └─────────┘                   │
                        │                                      │
                        v                                      │
                    ┌────────┐   ┌──────────┐                  │
                    │  core  │   │transport │                  │
                    │ rules, │   │ Local /  │                  │
                    │ pipe-  │   │ HTTP /   │                  │
                    │ line,  │   │ Veilid   │                  │
                    │ canvas │   │ (stub)   │                  │
                    └────────┘   └──────────┘                  │
                                                               │
                              Library Crates                   │
                                                               │
                    (Tauri desktop uses springtale-runtime ────┘
                     directly via IPC command handlers)
```

*Fig. 1. Crate dependency graph. Arrows point from dependent to dependency. `springtale-cooperation` has zero internal Springtale dependencies; `core` and `crypto` are the foundation everything else builds on.*

### 1.1. What Each Crate Does

**TABLE I. LIBRARY CRATES**

| Crate | One-line purpose |
|---|---|
| `springtale-core` | Rule engine, pipeline composition, event routing, data transforms, canvas types |
| `springtale-crypto` | Ed25519 keypairs, XChaCha20-Poly1305 vault, Argon2id KDF, manifest signatures, mlock |
| `springtale-transport` | `Transport` trait + Local (Unix socket), HTTP (rustls mTLS), Veilid (stub) impls |
| `springtale-connector` | `Connector` trait, Wasmtime WASM sandbox, manifest parser, capability system, registry, subscription lifecycle |
| `springtale-store` | SQLite backend (SQLite3MultipleCiphers) with WAL mode, declarative schema (`PRAGMA user_version`), AEAD-encrypted bot memory, cooperation + mental model schema |
| `springtale-scheduler` | Cron executor, filesystem watcher, job queue, heartbeat monitor, exponential backoff |
| `springtale-ai` | `AiAdapter` trait + Noop / Ollama / OpenAI-compat / Anthropic adapters + OWASP sanitiser + tool-calling (`ToolCall` / `ToolResult` / `ToolPolicy`) |
| `springtale-mcp` | MCP protocol bridge (`rmcp` 1.x) — wraps any `Connector` as an MCP server automatically. Handler module split; each handler owns its capability check |
| `springtale-sentinel` | Behavioural monitor, toxic-pair capability detection, audit trail |
| `springtale-cooperation` | Cooperation framework crate — 42 pub modules covering cadence, momentum, formations, rally, recovery, supervision, stigmergy, contract net, consensus, commit, interference, transformation, mental model, role dynamics, pacing, handoff, attention, awareness, authority, cross-formation gossip, persistent memory, and more. Zero internal Springtale dependencies. See [cooperation.md](cooperation.md) |
| `springtale-runtime` | Shared init / dispatch / operations layer used by both the daemon and the Tauri desktop app. Hosts `LiveFormationReader` trait for UI formation state |
| `springtale-bot` | Bot runtime, command router, handler registry, session memory, tool_runner, orchestrator (composer + intervention), and the 25-step cooperation tick pipeline |
| `springtale-wit` | G3 — WIT world for WASM Component Model embedding (Bevy, Unity, wasmCloud, custom hosts). Ships the `.wit` artifact only |
| `springtale-py` | G3 — pyo3 Python bindings (cdylib + rlib). Stable ABI works on Python 3.9+. Wrap with `maturin` to produce a distributable wheel |

---

## 2. How an Event Flows

When something happens — a Kick stream goes live, a file changes, a cron timer fires — here's the path through the system:

```
  External Service          springtaled                     Connector
  (Kick, GitHub,            Runtime                         Registry
   filesystem...)
       │                       │                               │
       │  1. webhook/poll/     │                               │
       │     watcher event     │                               │
       ├──────────────────────>│                               │
       │                       │                               │
       │                       │  2. create TriggerEvent       │
       │                       ├──────────┐                    │
       │                       │          v                    │
       │                       │   ┌─────────────┐            │
       │                       │   │ Rule Engine  │            │
       │                       │   │  evaluate()  │            │
       │                       │   └──────┬──────┘            │
       │                       │          │                    │
       │                       │   3. for each enabled rule:   │
       │                       │      does trigger match?      │
       │                       │          │                    │
       │                       │          v                    │
       │                       │   ┌─────────────┐            │
       │                       │   │ Conditions   │── no ──> skip
       │                       │   │ (all must    │            │
       │                       │   │  pass)       │            │
       │                       │   └──────┬──────┘            │
       │                       │          │ yes               │
       │                       │          v                    │
       │                       │   ┌─────────────┐            │
       │                       │   │  Pipeline    │            │
       │                       │   │  Stage 1..N  │            │
       │                       │   └──────┬──────┘            │
       │                       │          │                    │
       │                       │   4. enqueue Job              │
       │                       │          │                    │
       │                       │          v                    │
       │                       │   ┌─────────────┐  execute() │
       │                       │   │  Dispatch   ├───────────>│
       │                       │   └─────────────┘  5. cap    │
       │                       │                    check +   │
       │                       │                    run       │
       │                       │<────── 6. result ────────────┤
       │                       │                               │
       │                       │  7. log event to store        │
       │                       │                               │
```

*Fig. 2. Event flow from external trigger through rule evaluation to connector dispatch. Steps 4-6 are handled by the job queue with 4 concurrent workers.*

---

## 3. Where Data Lives

All data stays on your device. No cloud, no sync, no telemetry.

```
  ~/.local/share/springtale/
  ├── springtale.db          SQLite database (WAL mode, 0o600 permissions)
  │   ├── connectors         installed connector records
  │   ├── rules              rule definitions (trigger, conditions, actions)
  │   ├── events             event log (connector, trigger, payload, timestamp)
  │   └── jobs               job queue (pending, running, complete, failed)
  │
  ├── vault.bin              encrypted binary vault (XChaCha20-Poly1305)
  │   ├── Ed25519 keypair    node identity
  │   └── stored secrets     connector credentials
  │
  └── springtale.sock        Unix domain socket (local transport)
```

*Fig. 3. Default data layout. All paths configurable via `springtale.toml` or environment variables.*

The database uses SQLite's WAL mode for concurrent reads during write operations. File permissions are set to `0o600` (owner read/write only). The vault is encrypted with a key derived from your passphrase via Argon2id — no plaintext credential files ever touch disk.

---

## 4. What's Pluggable

Three core traits define the extension points. Each has a working default and planned future implementations:

```
  ┌──────────────────────────────────────────────────────────────┐
  │                    springtaled runtime                       │
  │                                                              │
  │  ┌─────────────────────────────────────────────────────────┐ │
  │  │ Transport trait                                         │ │
  │  │                                                         │ │
  │  │   LocalTransport ──── Unix domain socket (present)      │ │
  │  │   HttpTransport  ──── rustls mTLS (present)             │ │
  │  │   VeilidTransport ─── stub, returns NotConnected        │ │
  │  └─────────────────────────────────────────────────────────┘ │
  │                                                              │
  │  ┌─────────────────────────────────────────────────────────┐ │
  │  │ AiAdapter trait                                         │ │
  │  │                                                         │ │
  │  │   NoopAdapter         ── default, no AI                  │ │
  │  │   OllamaAdapter       ── local models (NDJSON stream)    │ │
  │  │   AnthropicAdapter    ── Claude (SSE stream)              │ │
  │  │   OpenAiCompatAdapter ── OpenAI/Gemini/DeepSeek (SSE stream)│ │
  │  │   (hot-swappable at runtime via POST /config/ai)         │ │
  │  └──────────────────────────────────────────────────────────┘ │
  │                                                               │
  │  ┌──────────────────────────────────────────────────────────┐ │
  │  │ Connector trait                                          │ │
  │  │                                                          │ │
  │  │   NativeConnector ── in-process (15 connectors present)  │ │
  │  │   WasmConnector   ── sandbox built; no connector uses it │ │
  │  │                                                          │ │
  │  │   connector-matrix not in workspace (upstream CVE).      │ │
  │  └──────────────────────────────────────────────────────────┘ │
  └──────────────────────────────────────────────────────────────┘
```

*Fig. 4. Pluggable trait boundaries. Swap any implementation without changing business logic. The `NoopAdapter` proves the entire platform works with zero AI.*

---

## 5. The Applications

### 5.1. springtaled (Daemon)

The headless daemon that runs the show. Boot is a 9-step ordered pipeline split between `apps/springtaled/src/runtime/boot/` (daemon-specific) and `crates/springtale-runtime/src/init.rs` (shared with the Tauri desktop app):

```
  1. Load config ─────> springtale.toml + env overrides
         │              rustls crypto provider install
         │              tracing subscriber
         │
  2. Init crypto ─────> acquire passphrase (file → env → tty)
         │              unlock or create vault
         │              derive API token via HMAC-SHA256
         │
  3. Shared runtime ──> springtale_runtime::init()
         │              • open store + apply declarative schema
         │              • load RuleEngine from store
         │              • start WASM engine + epoch ticker
         │              • discover connectors via inventory
         │              • init AI adapter (ArcSwap)
         │              • init sentinel + canvas bus
         │
  4. Transport ───────> Local / HTTP based on config
         │
  5. Schedulers ──────> CronExecutor, FsWatcher, heartbeat
         │
  6. Job queue ───────> JobProducer + consumer loop
         │
  7. Bot init ────────> wire chat connectors (telegram,
         │              nostr, irc, discord, slack, signal)
         │              install cooperation channel +
         │              LiveFormationReader
         │              spawn bot event loop
         │
  8. API server ──────> axum::build_router + bind
         │
  9. Ready ───────────> mark ready=true
                        start API + trigger event loop
                        wait on shutdown signal
```

*Fig. 5. Daemon boot sequence. See [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md) §3 for file:line refs.*

Exposes ~80 REST endpoints for connector management, rule CRUD, formation lifecycle (deploy / pause / resume / dissolve / rally / intent / members / toggle-guard / cycle-autonomy), canvas updates, event streaming, configuration, webhook ingestion, diagnostics, onboarding, templates, fixes, per-agent autonomy, author keys, bot admin, memory audit/compact, data export, and send/execute. See [reference/api.md](../reference/api.md) for the full endpoint catalogue.

### 5.2. springtale-cli (Terminal)

The CLI for local configuration and management:

```
  springtale init                         create vault + database
  springtale new <template>               scaffold a project from a template
  springtale server start                 start daemon inline
  springtale doctor                       run diagnostic checks
  springtale fix <error-id>               apply an auto-repair suggestion
  springtale trace <connector> <rule>     debug trace execution
  springtale connector <subcmd>           install/list/config/test/enable/disable
  springtale rule <subcmd>                create/list/delete/test/enable/disable
  springtale events --limit 50            query event log
  springtale agent <subcmd>               status/list
  springtale sessions list                list bot sessions
  springtale vault duress-setup           configure decoy vault
  springtale crypto rotate-vault-key      rotate the vault KEK
  springtale bot pair-init                pair-init subcommand
  springtale bot panic-unpair             forcibly unpair
  springtale travel prepare|restore       wipe/restore for device seizure
  springtale memory audit|compact         inspect/trim bot memory
  springtale data export                  export user data
  springtale panic                        emergency wipe (< 3 s)
```

Output defaults to formatted tables. Pass `--json` for machine-readable output. See [reference/cli.md](../reference/cli.md) for full details.

---

## 6. The Cooperation Tick

Every active formation runs a **25-step tick pipeline** when the cadence bus fires. `springtale-bot::runtime::event_loop::handle_cadence_tick` acquires the locks and loops over formations; the pipeline itself is `springtale-bot::runtime::tick_steps::run_tick`, one named module per step:

```
  pacing gate  the divider skips bus ticks whose sequence the formation's
               current phase does not admit
  1.  build_reports          run every member's agent loop, collect reports
  2.  momentum decay         inactivity check
  3.  update momentum        success / interference / failure
  4.  liveness               mark members down or recovered
  5.  supervision            supervisor checks, rally events
  6.  fuel                   per-member fuel drain
  7.  implicit signals       derive signals from the tick's reports
  8.  state broadcast        member state out to the formation
  9.  persist momentum       → formation_momentum table
 10.  publish context        FormationContext to watching members
 11.  gossip awareness       awareness via the gossip substrate (Warming+)
 12.  log interference       interference events
 13.  check pacing           phase transition from the tick's stress sample
 14.  check cascade          cascade detection + self-rally
 15.  check interventions    L6 commander override
 16.  recovery               distress → helper selection
 17.  transformation         failing members swap role
 18.  replan (CBBA)          global task reallocation
 19.  resolve consensus      vote deadlines
 20.  tick commits           advance commit barriers
 21.  expire commits         completed or timed-out barriers
 22.  update mental model    from reports + interferences
 23.  orchestrate            decompose intent (Fever tier only)
 24.  publish formation view cross-formation gossip bus
 25.  emit canvas update     per-tick canvas summary — only when a canvas
                            sender is wired (skipped in headless builds)
```

Step 1 runs the agent-side loop for each member: `sense` → `inbox` → `react` (awareness only) → `scan`, in `springtale-cooperation::agent::step::*`. `respond_cfp` is not a fifth in-order step — it fires reactively from the runner when a call for proposals arrives. Two cooperation modules are not called from this pipeline: `agent_loop::AgentLoop::tick()` scaffolding, and `sacrifice`, which runs from the agent step module rather than the formation tick. See [guide/cooperation.md](cooperation.md) for a user-facing tour.

---

## 7. The Frontend

The Tauri shell and the web dashboard share `tauri/packages/ui`. The **colony canvas** renders running formations as an RTS-style ecosystem. Connector nodes, rules/agents as springtails, formations as zones, pipelines as mycelium lines. Live data flows through two paths:

- `/canvas/stream` SSE — delta updates to `CanvasState`
- `LiveFormationReader` trait (`springtale-runtime`) — enriched per-formation state (momentum, rally tokens, attention load, guard status, member health/liveness)

The desktop shell wraps these through Tauri IPC commands (27 modules in `tauri/apps/desktop/src-tauri/src/commands/`); the web dashboard wraps them through HTTP + SSE. Both sit behind the `DataProvider` abstraction so components don't care which transport they're on.

Formation selection in the canvas opens a command grid (DEPLOY / PAUSE / RESUME / REMOVE) wired to `/formations/*` endpoints. Member detail shows rally pips (Monster Hunter carts), attention distribution bar (Army of Two aggro meter), guard status badge, aggregate operational/load/fuel row, and per-member health + liveness icons.

Two themes ship: the original colony forest theme and a chiral diorama theme (default). Themes are CSS-only.

**Recipes** are a curated automation library — browseable bundles of rules + connector configs + AI prompt scaffolding that one click can install into a running daemon. The recipes UI (RecipeLibraryOverlay, RecipeCard, RecipeQuickView, RecipeDeployPanel, RecipeAuthorPanel) sits in the canvas overlay layer and talks to 16 `/recipes/*` HTTP endpoints (browse, favorite, fork, preflight, apply, render, preview, import/export TOML). User-saved recipes persist under `/recipes/user/{id}`.

**Cross-formation event ribbon** (EventRibbon component) streams the cooperation gossip bus into the canvas — sibling formations broadcast `FormationView` snapshots each tick, surfacing as a live ribbon along the bottom of the viewport.

**Safety panel** (SafetyPanel component) surfaces the disguise tray icon picker, quick-hide shortcut binding, panic-tap count, and duress vault status. Wired to `/safety/disguise/{active,profile}`, `/safety/panic_tap_count`, and the Tauri-side `quick_hide` + `tray` commands.

**Approval card** (ApprovalCard component) presents pending destructive-action approval requests from the sentinel gate (G5b). When wired, it replaces the headless `DefaultDenyApprovalGate` with an interactive prompt.

Full reference: [guide/colony-canvas.md](colony-canvas.md).

---

## 8. Known Gaps

The following areas diverge from the design intent in `docs/current-arch/`:

| Area | State |
|---|---|
| `connector-matrix` | Not in the workspace. `matrix-sdk` pins `rusqlite` 0.37 with an open heap-leak CVE; Springtale uses the patched 0.39. |
| WASM connectors | The Wasmtime host, capability gate, subscription lifecycle across the sandbox, and SDK all exist. All 15 first-party connectors are native Rust; no WASM connector rides the sandbox today. |
| Job queue | `JobProducer` is an in-memory mpsc sender. The `jobs` SQLite table and `StorageBackend` method signatures exist, but the persistent-queue backing is not wired. |
| `VeilidTransport` | Stub. Every method returns `TransportError::NotConnected`. |
| Formation → rules generation | Formations define intent; rules are still authored separately. Auto-derivation of rules from a formation's intent is not implemented. |
| i18n, a11y, visual rule builder | English-only. Screen-reader and keyboard-nav work is not yet done. Drag-and-drop rule builder not implemented. |

Full detail with rationale: [`docs/arch/AUDIT-NOTES.md`](../arch/AUDIT-NOTES.md). Delivery plan: [ROADMAP.md](../ROADMAP.md).

---

## References

- [1] As-built architecture with file:line refs: [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md)
- [2] Design intent + full threat model: [`docs/current-arch/ARCHITECTURE.md`](../current-arch/ARCHITECTURE.md)
- [3] Cooperation framework: [`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md)
- [4] Crate structure guidelines: `.claude/rules/backend/crate-structure.md`
- [5] Rust conventions: `.claude/rules/backend/rust-conventions.md`
