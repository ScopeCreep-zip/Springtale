# Roadmap

Springtale ships in five phases. Each phase builds on the last — no phase skips ahead.

## 1. Phase Overview

```
  Phase 1a         Phase 1b         Phase 2a         Phase 2b         Phase 3
  Framework        Bot              Chat + AI        Desktop +        Veilid
  + Connectors     Foundations                       Safety           Mesh
  ━━━━━━━━━━━━━    ━━━━━━━━━━━━     ━━━━━━━━━━━━     ━━━━━━━━━━━━     ━━━━━━━━━━
  ██████████████   ██████████████   ██████████░░     ████████░░░░     ░░░░░░░░░░
```

*Fig. 1. Phase timeline. Filled blocks indicate work present in the workspace.*

**TABLE I. PHASES**

| Phase | Name | Deliverables | State |
|---|---|---|---|
| 1a | Framework + Connectors | Daemon, CLI, 14 library crates, 8 baseline connectors (kick, presearch, bluesky, github, filesystem, shell, http, opencode), SQLite (declarative schema v1 in `schema/sql/`), crypto vault, WASM sandbox, MCP endpoint | Present. Connector roster grew to 15 first-party through Phases 1b/2a. |
| 1b | Bot Foundations | `springtale-bot`, command router (prefix / pattern / alias), cooperation framework, `connector-telegram`, session memory | Present. Cooperation framework extracted to its own `springtale-cooperation` crate (42 pub modules) and wired into a 25-step formation tick; see §3.2. |
| 2a | Chat + AI | Discord, Slack, IRC, Signal, Nostr connectors. Anthropic / Ollama / OpenAI-compat adapters (all three stream). `HttpTransport` (rustls mTLS). `springtale-sentinel`. Tool-calling across all AI adapters. | Present. Matrix is held on upstream `rusqlite` CVE. |
| 2b | Desktop + Safety | Tauri 2 shell, SolidJS dashboard + colony canvas (RTS formation visualisation), duress vault, panic wipe, travel mode. Visual rule builder, i18n, a11y. | Shell, dashboard, colony canvas (with formation command grid, rally pips, attention bar, liveness/health encoding), duress, panic wipe, travel mode present. Visual rule builder (`RuleBuilderOverlay`), i18n (eight locales), quick-hide, and lock-screen content protection present. a11y not implemented. |
| 3 | Veilid Mesh | `VeilidTransport`, P2P mesh, distributed registry, Rekindle integration | Not implemented. `VeilidTransport` exists as a stub — every method returns `TransportError::NotConnected`. |

---

## 2. Phase 1a — Framework + Connectors

The foundation. A single-binary daemon, CLI, rule engine, crypto vault, WASM sandbox, and seven first-party connectors — all working without any AI.

### 2.1. Deliverables

**TABLE II. PHASE 1A WORKSPACE MEMBERS**

| Crate | Type | Purpose |
|---|---|---|
| `springtale-core` | library | Pipeline composition, rule engine, router, transforms, canvas types |
| `springtale-crypto` | library | Ed25519 identity, vault (Argon2id + XChaCha20-Poly1305), manifest signatures |
| `springtale-transport` | library | Transport trait + Local (Unix socket) implementation |
| `springtale-connector` | library | Connector trait, WASM sandbox (Wasmtime), manifest verification, capability system, subscription lifecycle |
| `springtale-store` | library | SQLite backend (SQLite3MultipleCiphers) with WAL mode, declarative schema (`PRAGMA user_version`), AEAD-encrypted bot memory, cooperation schema |
| `springtale-scheduler` | library | Cron executor, file watcher, job queue, heartbeat monitor, retry with backoff |
| `springtale-ai` | library | AI adapter trait + NoopAdapter (default). Adapters added in Phase 2a |
| `springtale-mcp` | library | MCP protocol bridge (`rmcp` 1.x) — any connector becomes an MCP server |
| `springtale-runtime` | library | Shared runtime init, dispatch, operations layer, `LiveFormationReader` trait |
| `connector-kick` | connector | Kick streaming — OAuth 2.1 PKCE, chat, streams, webhooks |
| `connector-presearch` | connector | Presearch — search + scrape with caching |
| `connector-bluesky` | connector | Bluesky/ATProto — posts, likes, reposts, Jetstream firehose |
| `connector-github` | connector | GitHub — issues, comments, diffs, HMAC webhook verification |
| `connector-filesystem` | connector | Local filesystem — watch, read, write with path allow-lists |
| `connector-shell` | connector | Shell execution — command allow-list, timeout, direct process |
| `connector-http` | connector | Generic HTTP — GET/POST with host allow-list |
| `connector-opencode` | connector | OpenCode — agentic coding via local `opencode serve` daemon, approval-gated |
| `springtaled` | application | Headless daemon with REST API, webhook ingestion, job dispatch |
| `springtale-cli` | application | CLI for vault init, connector management, rule authoring, event queries |

### 2.2. Capabilities

- TOML-based rule authoring with trigger/condition/action composition
- Ed25519 keypair generation and encrypted vault storage
- WASM sandbox with 10M instruction fuel, 64MB memory, 30s timeout
- Manifest signing and verification
- Capability-based permission system with toxic pair detection
- RESTful management API with HMAC bearer auth and rate limiting
- Cron scheduling + filesystem watching + webhook ingestion
- MCP over Streamable HTTP at `/mcp`, covering the whole registry, behind the
  daemon's Origin check and bearer auth (`apps/springtaled/src/api/mcp.rs`)
- Docker deployment with hardened security (read-only root, drop all caps)
- CI pipeline: fmt, clippy, nextest, cargo-deny, cargo-audit, gitleaks

---

## 3. Phase 1b — Bot Foundations

Classical bot runtime with deterministic command routing. No AI needed — `/search tokyo weather` matches a handler, calls a connector, formats the result.

### 3.1. Deliverables

- `springtale-bot` — Command routing engine with prefix, pattern, alias, and fallback matching
- `springtale-cooperation` — Standalone crate housing the full 40-module cooperation framework (see §3.2)
- `connector-telegram` — First chat connector, Telegram Bot API (polling + webhooks)
- Event loop: four-way `tokio::select!` over connector events, rule-engine triggers, cadence ticks, and formation commands
- Session state: per-user, per-channel context in SQLite, AEAD-encrypted memory rows
- Orchestrator + cooperation framework: formations, momentum tiers, shared blackboard, intent patterns (see [`docs/intended-arch/COOPERATION.md`](intended-arch/COOPERATION.md) and [`docs/guide/cooperation.md`](guide/cooperation.md))
- Built-in handler registry, persona config, memory compaction
- Tool-calling loop with MCP handler split (AI adapters emit `ToolCall`, runtime re-enters capability gate per call)

### 3.2. Cooperation wiring state

In April 2026 the cooperation architecture was lifted out of `springtale-bot` into its own crate, `springtale-cooperation` (42 public modules). The crate has zero internal Springtale dependencies and is consumed by `springtale-bot` and `springtale-runtime`. See [`docs/guide/cooperation.md`](guide/cooperation.md) for a user-facing tour.

The formation tick runs a **25-step pipeline**. `springtale-bot::runtime::event_loop::handle_cadence_tick` is the driver — it takes the locks, calls `springtale-bot::runtime::tick_steps::run_tick` once per active formation, then runs the tail passes. `run_tick` is the pipeline, one named module per step:

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

Step 1 runs each member's agent loop: `sense` → `inbox` → `react` (awareness only) → `scan`. `respond_cfp` fires reactively when a call for proposals arrives, not at a fixed point in the tick.

Modules participating in the tick: `cadence`, `momentum`, `attention`, `awareness`, `mental_model`, `consensus`, `commit`, `interference`, `transformation`, `capability`, `rally`, `recovery`, `comms`, `handoff`, `pacing`, `supervision`, `stigmergy`, `contract_net`, `routing`, `role`, `dissemination`, `replan::cbba`, `tick_processor`, plus state / agent / authority / layer / utility support modules.

Not invoked from the formation tick: `agent_loop::AgentLoop::tick()` scaffolding, and `sacrifice`, which runs from the agent step module (`agent::step::sacrifice`) rather than from `run_tick`.

Full detail: [`docs/arch/AUDIT-NOTES.md §3`](arch/AUDIT-NOTES.md).

```
  User Message                 springtale-bot                  Connector
       │                            │                             │
       │  "/search tokyo weather"   │                             │
       ├───────────────────────────>│                             │
       │                            │  prefix match: /search      │
       │                            ├────────┐                    │
       │                            │        v                    │
       │                            │  ┌───────────┐             │
       │                            │  │  Handler   │             │
       │                            │  │  Registry  │             │
       │                            │  └─────┬─────┘             │
       │                            │        │  execute()         │
       │                            │        ├───────────────────>│
       │                            │        │                    │
       │                            │<────── result ──────────────┤
       │                            │                             │
       │  "Weather in Tokyo: 18°C"  │                             │
       │<───────────────────────────┤                             │
```

*Fig. 2. Bot command routing. Deterministic prefix matching, no AI required.*

---

## 4. Phase 2a — Chat Coverage + AI Adapters

Broad chat platform support and optional AI integration.

### 4.1. Connectors

**TABLE III. PHASE 2A CONNECTORS**

| Connector | Platform | Transport | State |
|---|---|---|---|
| `connector-discord` | Discord | twilight-gateway WebSocket | Present |
| `connector-signal` | Signal | signal-cli bridge | Present |
| `connector-irc` | IRC | native IRC client (TCP/TLS) | Present |
| `connector-slack` | Slack | Socket Mode + webhooks | Present |
| `connector-nostr` | Nostr | relay WebSocket + NIP-44 | Present |
| `connector-browser` | Headless browser | Chromium via CDP | Present |
| `connector-matrix` | Matrix | matrix-sdk | Not in workspace. Held on `matrix-sdk`'s pinned `rusqlite` 0.37 (CVE-2025-70873). Springtale uses the patched 0.39. |

### 4.2. AI Integration

AI is optional — it's a pipeline action and a bot fallback parser, never a requirement. The `NoopAdapter` default means nothing breaks when no AI is configured.

**TABLE IV. AI ADAPTERS**

| Adapter | Streaming | State |
|---|---|---|
| `NoopAdapter` (default) | — | Present |
| `OllamaAdapter` (local models) | NDJSON | Present, full streaming |
| `AnthropicAdapter` (Claude API) | SSE | Present, full streaming |
| `OpenAiCompatAdapter` (OpenAI / Gemini / DeepSeek / llama.cpp) | SSE | Present, full streaming. Tool calling routed through `complete_with_tools()` (non-streaming) since argument JSON must be complete before tool execution. |

Adapters are hot-swappable at runtime via `POST /config/ai`. Two-layer prompt sanitisation: closed `AiRequest` enum (compile-time) + OWASP-pattern scanner (runtime) detects PII, credentials, prompt injection, suspicious encoding.

Natural-language-to-rule conversion: `POST /rules/parse` sends a description to the configured AI adapter and returns a ready-to-store rule.

### 4.3. Other Deliverables

- `springtale-sentinel` — present. Behavioural monitor evaluates every action in dispatch (circuit breaker, rate limiter, dead-man switch, destructive action gate). Toxic-pair detection runs at manifest install time. Audit trail in `audit_trail` table.
- `HttpTransport` — present. rustls mTLS server and client via `axum-server` + `reqwest`.
- Tool-calling — present across Anthropic, Ollama, and OpenAI-compat adapters. `springtale-ai::ToolCall` emitted by adapters; `springtale-bot::tool_runner` re-enters the capability gate on every call. MCP handler module split so each handler owns its capability check.
- Recursive orchestration — present. Fever-tier formations invoke the AI adapter for intent decomposition; sub-tasks land on the shared blackboard; members pull via the agent loop (`sense → scan → react → respond_cfp → inbox`). Role transformation, rally, recovery, consensus, commit, interference, and pacing are fully wired — see §3.2 pipeline map.

---

## 5. Phase 2b — Desktop + Safety

Tauri 2 desktop shell with a SolidJS frontend that renders an RTS-inspired colony view of running connectors, rules, and agent formations. Safety features for people in dangerous situations.

### 5.1. Present

- **Tauri 2 desktop shell** — `tauri/apps/desktop`, a **sidecar client**. It does
  not host the runtime: on unlock it spawns `springtaled` as a Tauri sidecar
  (`--bind 127.0.0.1:0 --passphrase-stdin`, passphrase over stdin, waits for
  `READY {port}`), and the frontend then talks to it over HTTP on loopback
  through the same provider the dashboard uses. The API token is derived from
  the passphrase on both sides and never transmitted. The desktop crate depends
  on only `springtale-crypto` and `springtale-transport`, and exposes five Tauri
  IPC command modules for what the OS owns: `vault`, `tray`, `quick_hide`,
  `safety`, `selector_picker`. The only local state it keeps is
  `{data_dir}/shell-prefs.json` (mode 0600, holding `window_title` so the
  disguise name is right on the first frame of a cold start) plus the unlocked
  vault and the auto-lock timer in memory.
- **Web dashboard** — `tauri/apps/dashboard`. SPA served by `springtaled`, bearer-token auth, SSE for live event and canvas streams.
- **Shared component library** — `tauri/packages/ui` with `DataProvider` abstraction. Desktop wraps Tauri IPC; web wraps HTTP + SSE.
- **Colony canvas** — RTS-style pixel-art ecosystem view: connectors → nodes, rules/agents → springtails, formations → zones, pipelines → mycelium. Live state over `/canvas/stream` (SSE) + `LiveFormationReader` for formation detail. Formation command grid (DEPLOY / PAUSE / RESUME / REMOVE), rally pips (Monster Hunter carts), attention distribution bar (Army of Two aggro), guard status badge, agent liveness / health encoding. See [`docs/guide/colony-canvas.md`](guide/colony-canvas.md).
- **Chiral diorama theme** — default Tauri desktop theme; coexists with the original colony theme. Selected via settings.
- **Duress passphrase** — dual encrypted regions, constant 131,152-byte file size, VeraCrypt-style plausible deniability.
- **Panic wipe** — single-pass random overwrite + fsync + unlink, <3 s on 1 MB vault.
- **Travel mode** — `springtale travel prepare --backup-to` and `travel restore --from`.
- **MCP endpoint** — Model Context Protocol is served by **the daemon**, over the whole connector registry, at an authenticated loopback endpoint. `springtale-mcp` exposes `SpringtaleMcp::new(runtime)` (every installed connector) and `SpringtaleMcp::for_connector` (one), and `springtaled` mounts it at `/mcp` as a Streamable HTTP service (`apps/springtaled/src/api/mcp.rs`). Requests pass `auth::require_local_origin` and then the daemon's own bearer check on *every* request; `Mcp-Session-Id` is a transport correlator, never authentication. Tool calls dispatch through the same sentinel, approval gate and executions recorder as a rule action. The per-connector stdio subprocess transport is gone — there is no `mcp` CLI subcommand, and none is needed.
- **Visual rule builder** — `tauri/packages/ui/src/colony/RuleBuilderOverlay.tsx`, a guided
  multi-step form (trigger → condition → action → preview), wired into both the desktop
  and dashboard apps. Not drag-and-drop.
- **Quick-hide** — OS-wide global shortcut that hides the window
  (`tauri/apps/desktop/src-tauri/src/commands/quick_hide.rs`), with a fallback combo;
  the shortcut is persisted in the `safety` table.
- **Content protection** — `window.set_content_protected` blocks screen capture
  (`commands/safety.rs`). Tauri 2 does not support it on Linux; the command returns an
  error there.
- **App disguise** — tray icon and app name swap across four icon profiles
  (`commands/tray.rs`, `POST /safety/disguise/profile`). It disguises the tray entry, not
  the window contents.
- **i18n** — 8 locales (en, es, pt, fr, ar, th, tl, ja) via `@solid-primitives/i18n`,
  RTL-aware, switchable from app settings.
- **Accessibility** — skip link, `aria-live` regions on the safety panel, event ribbon,
  rule builder, canvas, travel mode and sessions; screen-reader navigation over the
  canvas; `prefers-reduced-motion` and high-contrast styles in `theme.css`.

### 5.2. Not Implemented

- Mobile (iOS + Android) via Tauri mobile
- User-controlled font scaling (the rest of the accessibility work listed in §5.1 has
  landed; font scaling is the one item with no implementation)

---

## 6. Phase 3 — Veilid Mesh

Not implemented. `VeilidTransport` exists in `crates/springtale-transport/src/veilid/stub.rs` with a private constructor and all methods returning `TransportError::NotConnected`.

Swap `LocalTransport` for `VeilidTransport`. One config change, zero business logic changes.

```
  ┌──────────┐                              ┌──────────┐
  │  Node A  │                              │  Node B  │
  │          │     Veilid Private Routes    │          │
  │ springtale│◄────────────────────────────►│springtale│
  │  daemon  │     E2E encrypted            │  daemon  │
  │          │     No IP leakage            │          │
  └────┬─────┘     No central server        └────┬─────┘
       │                                         │
       v                                         v
  ┌──────────┐                              ┌──────────┐
  │  Local   │                              │  Local   │
  │  SQLite  │                              │  SQLite  │
  │  + Vault │                              │  + Vault │
  └──────────┘                              └──────────┘
```

*Fig. 3. Phase 3 P2P topology. Each node is self-contained with local storage and an encrypted vault. Communication happens over Veilid private routes — no server, no IP exposure.*

### 6.1. Key Capabilities

- Bots join Rekindle communities as headless Veilid members
- Per-community HKDF pseudonyms — unlinkable identities across contexts
- Connector registry moves from SQLite to DHT
- E2E encrypted AI chat: conversation exists only on your device and the bot's device

---

## References

- [1] As-built architecture: [`docs/arch/ARCHITECTURE.md`](arch/ARCHITECTURE.md)
- [2] As-built security posture: [`docs/arch/SECURITY.md`](arch/SECURITY.md)
- [3] Known drift + in-flight work: [`docs/arch/AUDIT-NOTES.md`](arch/AUDIT-NOTES.md)
- [4] Design intent + threat model: [`docs/current-arch/ARCHITECTURE.md`](current-arch/ARCHITECTURE.md)
- [5] Cooperation framework spec: [`docs/intended-arch/COOPERATION.md`](intended-arch/COOPERATION.md)
- [6] Rekindle P2P protocol: [`docs/current-arch/rekindle-architecture.md`](current-arch/rekindle-architecture.md)
