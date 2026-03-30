# Roadmap

Springtale ships in five phases. Each phase builds on the last — no phase skips ahead.

## 1. Phase Overview

```
  Phase 1a         Phase 1b         Phase 2a         Phase 2b         Phase 3
  Framework        Bot              Chat + AI        Desktop +        Veilid
  + Connectors     Foundations                       Mobile + Safety  Mesh
  ━━━━━━━━━━━━━    ━━━━━━━━━━━━     ━━━━━━━━━━━━     ━━━━━━━━━━━━     ━━━━━━━━━━
  ██████████████    ░░░░░░░░░░░░     ░░░░░░░░░░░░     ░░░░░░░░░░░░     ░░░░░░░░░░
  COMPLETE         IN DESIGN        PLANNED          PLANNED          PLANNED
  ~20K LOC
```

*Fig. 1. Phase timeline. Filled blocks indicate completion.*

**TABLE I. PHASE STATUS**

| Phase | Name | Status | Key Deliverables |
|-------|------|--------|-----------------|
| 1a | Framework + Connectors | **COMPLETE** | Daemon, CLI, 8 crates, 7 connectors, SQLite, crypto vault, WASM sandbox |
| 1b | Bot Foundations | IN DESIGN | `springtale-bot` crate, classical command routing, `connector-telegram` |
| 2a | Chat + AI | PLANNED | 7 chat connectors, AI adapters, sentinel monitoring, `HttpTransport` |
| 2b | Desktop + Mobile + Safety | PLANNED | Tauri 2 shell, visual rule builder, duress/panic/travel mode, a11y |
| 3 | Veilid Mesh | PLANNED | `VeilidTransport`, P2P mesh, distributed registry, Rekindle integration |

---

## 2. Phase 1a — Framework + Connectors (COMPLETE)

The foundation. A single-binary daemon, CLI, rule engine, crypto vault, WASM sandbox, and seven first-party connectors — all working without any AI.

### 2.1. What Shipped

**TABLE II. PHASE 1A WORKSPACE MEMBERS**

| Crate | Type | Purpose |
|-------|------|---------|
| `springtale-core` | library | Pipeline composition, rule engine, router, transforms |
| `springtale-crypto` | library | Ed25519 identity, vault encryption, manifest signatures, capability tokens |
| `springtale-transport` | library | Transport trait + Unix socket implementation |
| `springtale-connector` | library | Connector trait, WASM sandbox, manifest verification, capability system |
| `springtale-store` | library | SQLite backend with WAL mode, schema, migrations |
| `springtale-scheduler` | library | Cron executor, file watcher, job queue, retry with backoff |
| `springtale-ai` | library | AI adapter trait + NoopAdapter (default) |
| `springtale-mcp` | library | MCP protocol bridge — any connector becomes an MCP server |
| `connector-kick` | connector | Kick streaming — OAuth 2.1 PKCE, chat, streams, webhooks |
| `connector-presearch` | connector | Presearch — search + scrape with caching |
| `connector-bluesky` | connector | Bluesky/ATProto — posts, likes, reposts, Jetstream firehose |
| `connector-github` | connector | GitHub — issues, comments, diffs, HMAC webhook verification |
| `connector-filesystem` | connector | Local filesystem — watch, read, write with path allow-lists |
| `connector-shell` | connector | Shell execution — command allow-list, timeout, direct process |
| `connector-http` | connector | Generic HTTP — GET/POST with host allow-list |
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

## 3. Phase 1b — Bot Foundations (IN DESIGN)

Classical bot runtime with deterministic command routing. No AI needed — `/search tokyo weather` matches a handler, calls a connector, formats the result.

### 3.1. Planned Deliverables

- `springtale-bot` — Command routing engine with prefix, pattern, and alias matching
- `connector-telegram` — First chat connector, Telegram Bot API
- Event loop: `tokio::select!` on connector events, rule events, scheduler events
- Session state: per-user, per-channel context
- Built-in commands: `/help`, `/status`, `/rules`, `/connectors`

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

## 4. Phase 2a — Chat Coverage + AI Adapters (PLANNED)

Broad chat platform support and optional AI integration.

### 4.1. Planned Connectors

| Connector | Platform | Transport |
|-----------|----------|-----------|
| `connector-discord` | Discord | WebSocket gateway |
| `connector-signal` | Signal | Signal Protocol |
| `connector-whatsapp` | WhatsApp | WhatsApp Business API |
| `connector-matrix` | Matrix | Matrix CS API |
| `connector-irc` | IRC | TCP/TLS |
| `connector-slack` | Slack | Events API |
| `connector-nostr` | Nostr | Relay WebSocket |

### 4.2. AI Integration

AI is optional — it's a pipeline action, not a requirement. Planned adapters:

- **Ollama** — Local models, no data leaves the device
- **OpenAI-compatible** — Any API matching the OpenAI spec
- **Anthropic** — Claude API
- **Voice** — STT/TTS for accessibility

Natural language to rule conversion: describe what you want, get a TOML rule back.

### 4.3. Other Deliverables

- `springtale-sentinel` — Runtime behavioral monitoring (rate limiter, circuit breaker, dead-man switch)
- `HttpTransport` — HTTP-based inter-node communication
- Recursive pipeline orchestration with subagent support

---

## 5. Phase 2b — Desktop + Mobile + Safety (PLANNED)

Tauri 2 shell across macOS, Windows, Linux, iOS, and Android. Safety features for people in dangerous situations.

### 5.1. Safety Features

- **Duress passphrase** — Unlocks a decoy vault under coercion
- **Panic wipe** — 3-second vault destruction (CLI, gesture, shake, dead-man timer)
- **Travel mode** — Pre-departure wipe + encrypted backup restore at destination
- **App disguise** — Configurable icon and name to avoid recognition
- **Quick-hide** — Instant dismiss to hide from over-the-shoulder observers

### 5.2. Other Deliverables

- Visual rule builder (drag-and-drop trigger/condition/action)
- Accessibility: screen readers (WAI-ARIA), keyboard nav, high contrast, font scaling
- i18n: English, Spanish, Portuguese, French, Arabic, Thai, Tagalog, Japanese

---

## 6. Phase 3 — Veilid Mesh (PLANNED)

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

- [1] Full architecture specification: [`docs/current-arch/ARCHITECTURE.md`](current-arch/ARCHITECTURE.md)
- [2] Security model and compliance mappings: [`docs/current-arch/SECURITY.md`](current-arch/SECURITY.md)
- [3] Rekindle P2P protocol: [`docs/current-arch/rekindle-architecture.md`](current-arch/rekindle-architecture.md)
- [4] Audit findings: [`docs/current-arch/AUDIT-NOTES.md`](current-arch/AUDIT-NOTES.md)
