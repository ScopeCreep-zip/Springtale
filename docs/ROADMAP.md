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
| 1a | Framework + Connectors | Daemon, CLI, 14 library crates, 8 baseline connectors (kick, presearch, bluesky, github, filesystem, shell, http, opencode), SQLite (declarative schema v1 in `schema/sql/`), crypto vault, WASM sandbox, MCP bridge | Present. Connector roster grew to 15 first-party through Phases 1b/2a. |
| 1b | Bot Foundations | `springtale-bot`, command router (prefix / pattern / alias), cooperation framework, `connector-telegram`, session memory | Present. Cooperation framework extracted to its own `springtale-cooperation` crate (40 pub modules) and wired into a 14-step formation tick; see §3.2. |
| 2a | Chat + AI | Discord, Slack, IRC, Signal, Nostr connectors. Anthropic / Ollama / OpenAI-compat adapters (all three stream). `HttpTransport` (rustls mTLS). `springtale-sentinel`. Tool-calling across all AI adapters. | Present. Matrix is held on upstream `rusqlite` CVE. |
| 2b | Desktop + Safety | Tauri 2 shell, SolidJS dashboard + colony canvas (RTS formation visualisation), duress vault, panic wipe, travel mode. Visual rule builder, i18n, a11y. | Shell, dashboard, colony canvas (with formation command grid, rally pips, attention bar, liveness/health encoding), duress, panic wipe, travel mode present. Visual rule builder, i18n, a11y not implemented. |
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
- Any connector auto-exposed as MCP server via stdio
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

In April 2026 the cooperation architecture was lifted out of `springtale-bot` into its own crate, `springtale-cooperation` (37 public modules). The crate has zero internal Springtale dependencies and is consumed by `springtale-bot` and `springtale-runtime`. See [`docs/guide/cooperation.md`](guide/cooperation.md) for a user-facing tour.

The formation tick runs a **14-step pipeline** inside `springtale-bot::runtime::event_loop::handle_cadence_tick`:

```
  1.  per-agent loop (sense / scan / react / respond-cfp / inbox)
  1b. drain async tick reports from cadence reports channel
  2.  tick_processor — action records + interference detection
  2b. rally supervisor drain (member outcomes → rally events)
  3.  momentum decay check (inactivity)
  4.  momentum update (success / interference / failure)
  4a-h. liveness, supervisor checks, per-member fuel, implicit signals,
        state broadcasts, cohesion signals
  5.  persist momentum row to SQLite
  6.  broadcast FormationContext to watchers
  7.  update awareness via gossip substrate (Warming+ only)
  7b. log interference events
  8.  evaluate pacing phase transitions
  9.  cascade detection + self-rally
  9b. recovery evaluation (distress → helper selection)
  10. role transformation for failing members
  11. check consensus deadlines
  12. expire completed / timed-out commit barriers
  13. update mental model from observed reports + interferences
  14. orchestrate — decompose intent into sub-tasks (Fever tier only)
```

**Every cooperation module is wired.** Modules participating in the tick: `cadence`, `momentum`, `attention`, `awareness`, `mental_model`, `consensus`, `commit`, `interference`, `transformation`, `capability`, `rally`, `recovery`, `sacrifice`, `comms`, `handoff`, `pacing`, `supervision`, `stigmergy`, `contract_net`, `routing`, `role`, `dissemination`, `tick_processor`, plus state / agent / authority / layer / utility / replan support modules.

Not invoked from the bot hot path (deliberate — they are ancillary helpers or consumed elsewhere): `agent_loop::AgentLoop::tick()` scaffolding, `replan::cbba` (wired only when orchestrator is present and needs a global replan).

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

- **Tauri 2 desktop shell** — `tauri/apps/desktop`. IPC via `invoke()` into the shared `springtale-runtime` crate. 33 command modules (agent, approval, authors, bot, canvas, chat, config, connectors, cooperation, data, diagnostics, drift, events, executions, fixes, formations, heartbeat, memory, onboarding, panic, quick_hide, recipes, rules, safety, selector_picker, send, sessions, templates, test_step, travel, tray, vault, workspaces, plus runtime_guard).
- **Web dashboard** — `tauri/apps/dashboard`. SPA served by `springtaled`, bearer-token auth, SSE for live event and canvas streams.
- **Shared component library** — `tauri/packages/ui` with `DataProvider` abstraction. Desktop wraps Tauri IPC; web wraps HTTP + SSE.
- **Colony canvas** — RTS-style pixel-art ecosystem view: connectors → nodes, rules/agents → springtails, formations → zones, pipelines → mycelium. Live state over `/canvas/stream` (SSE) + `LiveFormationReader` for formation detail. Formation command grid (DEPLOY / PAUSE / RESUME / REMOVE), rally pips (Monster Hunter carts), attention distribution bar (Army of Two aggro), guard status badge, agent liveness / health encoding. See [`docs/guide/colony-canvas.md`](guide/colony-canvas.md).
- **Chiral diorama theme** — default Tauri desktop theme; coexists with the original colony theme. Selected via settings.
- **Duress passphrase** — dual encrypted regions, constant 131,152-byte file size, VeraCrypt-style plausible deniability.
- **Panic wipe** — single-pass random overwrite + fsync + unlink, <3 s on 1 MB vault.
- **Travel mode** — `springtale travel prepare --backup-to` and `travel restore --from`.

### 5.2. Not Implemented

- Visual rule builder (drag-and-drop trigger / condition / action)
- Accessibility: screen readers (WAI-ARIA), keyboard nav, high contrast, font scaling
- i18n: English, Spanish, Portuguese, French, Arabic, Thai, Tagalog, Japanese
- Mobile (iOS + Android) via Tauri mobile
- App disguise, quick-hide gesture, lock-screen content protection

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
