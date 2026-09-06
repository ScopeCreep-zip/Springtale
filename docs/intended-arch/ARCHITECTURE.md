# Springtale — Architecture Document
> **Status:** Active Design · **Phase:** 2 of 3 (1a/1b shipped, 2a/2b in progress, 3 stub) · **Updated:** 2026-05-11 · **Version:** 2.1
>
> This is the **forward-looking design spec**. For the as-built snapshot
> (what's actually in the working tree right now), read
> [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md). Where this doc
> describes a feature that has shipped, the line-level details may have
> moved — trust `arch/` over `intended-arch/` for current code paths.

---

## Table of Contents

1. [Mission & Philosophy](#1-mission--philosophy)
2. [Security Model](#2-security-model)
3. [Phase Roadmap](#3-phase-roadmap)
4. [Workspace Layout](#4-workspace-layout)
5. [Cargo Workspace Dependencies](#5-cargo-workspace-dependencies)
6. [Core Crates](#6-core-crates)
7. [Connector Crates](#7-connector-crates)
8. [Applications](#8-applications)
9. [Tauri Desktop & Mobile Shell](#9-tauri-desktop--mobile-shell)
10. [TypeScript Connector SDK](#10-typescript-connector-sdk)
11. [Transport Abstraction & Phase Plan](#11-transport-abstraction--phase-plan)
12. [Dev Environment](#12-dev-environment)
13. [Ecosystem & Prior Art](#13-ecosystem--prior-art)
14. [Bot & Agent Framework](#14-bot--agent-framework)
15. [Proactive Architecture](#15-proactive-architecture)

**Companion document:** [SECURITY.md](SECURITY.md) — competitive analysis,
compliance mappings (OWASP ASVS, MITRE ATT&CK/ATLAS), CI pipeline specs,
dependency supply chain audit, Privacy by Design principles mapping.

---

## 1. Mission & Philosophy

Springtale is a **local-first, privacy-preserving automation platform**
built to outlast the AI hype cycle. It is connector infrastructure first, AI consumer second.

### Core Principles

**Security and privacy are not features — they are constraints.**
Every architectural decision is evaluated against the threat model in §2 before
it is accepted. Convenience never overrides security.

**Modules over inline items.**
All functions, types, error variants, and constants live in named modules.
No free-floating `impl` blocks at crate root. No inline type aliases in function
signatures. Every public surface is deliberate.

**Secrets are types, not strings.**
`Secret<T>` from the `secrecy` crate wraps all sensitive values. The type system
prevents accidental logging, cloning into unprotected memory, or passing to
functions that don't need exposure. Memory is zeroed on drop via `zeroize`.

**The AI adapter is a socket, not the foundation.**
The entire platform operates correctly with `AiAdapter = NoopAdapter`. Users
bring their own AI — a local Ollama instance, an OpenAI key, a Claude key —
and plug it into the bot. We provide the socket (`AiAdapter` trait), not the AI.
When the AI bubble pops, users unplug the adapter. Nothing breaks.

**The transport is swappable.**
All inter-node communication routes through the `Transport` trait. Phase 1
uses local Unix sockets. Phase 2 uses HTTP. Phase 3 drops in Veilid when Rekindle's
mesh is production-ready. No business logic changes between phases.

**Connectors are untrusted by default.**
Community connectors run inside a Wasmtime WASM Component Model sandbox.
They cannot access the filesystem, network, or keychain beyond what the
signed manifest explicitly declares and the user explicitly approves.

---

## 2. Security Model

### 2.1 Threat Model

| Threat | Severity | Mitigation |
|---|---|---|
| Malicious community connector | Critical | Wasmtime sandbox, manifest signing, capability allow-list |
| Supply chain compromise (npm) | Critical | Rust for all trusted code; TS only in sandboxed guest WASM |
| Secret leakage via logs | High | `Secret<T>` wrapper; `secrecy` prevents `Debug`/`Display` |
| Secret leakage via heap dump | High | `zeroize` zeroes memory on drop |
| Connector privilege escalation | High | Capability grants checked at runtime by host; guest cannot self-elevate |
| Man-in-the-middle on transport | High | All transport encrypted; `rustls-tls`, no `native-tls`/OpenSSL |
| Unsigned/tampered connectors | High | Ed25519 manifest signatures verified before load |
| Connector impersonation | Medium | Author pubkey pinned in manifest; registry maintains key→author map |
| Connector DoS (CPU/memory) | Medium | Wasmtime fuel metering + memory limits per sandbox instance |
| Credential exfiltration via connector | Medium | `network:outbound` capability must list exact hosts; wildcard disallowed |

### 2.2 Manifest Signing Flow

```
Connector author                  Registry                    User runtime
      │                               │                             │
      │  sign manifest with           │                             │
      │  ed25519 private key          │                             │
      │──────────────────────────────▶│                             │
      │                               │  store manifest +           │
      │                               │  signature + pubkey         │
      │                               │─────────────────────────────▶
      │                               │                             │ verify_signature()
      │                               │                             │ verify_capabilities()
      │                               │                             │ prompt user approval
      │                               │                             │ load into sandbox
```

### 2.3 Capability Grant Model

Capabilities are an explicit allow-list in the connector manifest.
The runtime grants only what is declared. Declaration ≠ approval —
the user must approve high-privilege capabilities at install time.

```
Capability::NetworkOutbound { host: "api.kick.com" }   // single host, no wildcards
Capability::FilesystemRead  { path: "~/.config/springtale" }
Capability::FilesystemWrite { path: "~/springtale-output" }
Capability::KeychainRead    { key:  "kick_oauth_token" }
Capability::ShellExec                                   // requires explicit user approval UI
```

`ShellExec` triggers a blocking approval modal in the Tauri shell before
the connector is permitted to load. This cannot be bypassed by the connector.

### 2.4 Secret Handling Rules

1. All `Secret<T>` values are created at config parse time and never unwrapped
   except at the precise call site that requires the raw value.
2. `.expose_secret()` call sites are annotated with `// SECURITY: expose needed for X`.
3. No `Secret<T>` value may appear in a struct that derives `Debug` without
   the field being wrapped in `secrecy`'s redacted display.
4. All network clients use `rustls-tls` exclusively. `native-tls` is banned
   via `Cargo.toml` `[patch]` to prevent transitive pulls.
5. TLS certificate validation is never disabled in any code path.

---

## 3. Phase Roadmap


### Phase 1a — Framework + Connectors (NosytLabs Obsolescence)

**Goal:** Ship a typed, signed, sandboxed connector framework with MCP compatibility
and a first-party connector library that covers and exceeds NosytLabs' entire catalog.

**What ships:** Single binary (`springtaled`) + CLI (`springtale-cli`) + Docker Compose.
**Storage:** SQLite (single file, zero external dependencies).
**Transport:** `LocalTransport` (same-machine, tokio Unix sockets)
**AI:** `NoopAdapter` — Phase 1a is a deterministic automation engine, no AI required.
**Rule authoring:** TOML files, hand-authored or imported.

**User experience in Phase 1a:**
1. `cargo install springtale-cli` (or Docker)
2. `springtale init` — creates `~/.local/share/springtale/` with SQLite DB + vault
3. `springtale connector install ./connector-kick.toml` — verify signature, approve capabilities, load
4. Write a rule in TOML: `rules/kick-announce.toml`
5. `springtale rule add --file rules/kick-announce.toml`
6. `springtale server start` — daemon runs rules, evaluates triggers, dispatches actions
7. Rules execute deterministically. No AI. No cloud. Single process, single file.

**Connectors shipping in Phase 1a:**
- `connector-kick` (replaces KickMCP)
- `connector-presearch` (replaces presearch-search-api-mcp)
- `connector-bluesky` (extends malwarevangelist-bot ATProto work)
- `connector-github`
- `connector-filesystem`
- `connector-shell`
- `connector-http`

**Crates shipping in Phase 1a:**
`springtale-core`, `springtale-crypto`, `springtale-transport` (LocalTransport only),
`springtale-connector`, `springtale-scheduler`, `springtale-store` (SQLite backend only),
`springtale-ai` (NoopAdapter only), `springtale-mcp`.

### Phase 1b — Bot Foundations (Bridge to OpenClaw Kill)

**Goal:** First chat connector. Classical bot runtime with deterministic command
routing. This is where Springtale becomes something OpenClaw users can switch to —
a bot you talk to through your existing chat platform, safely.

**What ships:** `springtale-bot` crate + `connector-telegram` (first chat connector).
**AI:** Still `NoopAdapter` default. Bot works entirely on classical command matching.

**Why classical bot architecture:**
OpenClaw routes every user interaction through an LLM. User says "search tokyo weather"
→ LLM decides to call the search tool → waits for API response → LLM formats the result.
Three API calls, latency, cost, and a runtime dependency on a company staying in business.

Springtale matches `/search tokyo weather` → calls connector-presearch → formats result
from a template. One call, instant, free, works forever. This is the same pattern IRC bots
have used since the 1990s, Discord bots since 2016, Telegram bots since 2015. The security
properties are well-understood. The failure modes are well-understood. There are no novel
attack surfaces in deterministic command routing.

AI becomes a fallback parser in Phase 2a: when no command matches and the user is having
a freeform conversation, the AI adapter gets a turn. When the AI bubble pops, you lose
the conversational interface. You keep every command, every scheduled task, every
automation, every connector. Nothing breaks.

**`springtale-bot` architecture (classical bot pattern):**
```
Message arrives from connector-telegram
│
├── Command Router (deterministic, no AI)
│   ├── Prefix match: /search, /help, /remind, /status
│   ├── Pattern match: regex triggers, keyword detection
│   ├── Alias resolution: user-defined command shortcuts
│   └── If no match → queue for AI fallback (Phase 2a) or "unknown command" response
│
├── Handler dispatch
│   ├── Handler has access to: connector actions, conversation state, user prefs, rule engine
│   ├── Handler calls connectors through springtale-connector sandbox (same security model)
│   └── Handler generates response from templates (no AI needed)
│
├── State management
│   ├── Conversation context (per-user, per-channel, SQLite-backed)
│   ├── User preferences (timezone, language, notification settings)
│   └── Session tracking (what the bot last said, what it's waiting for)
│
└── Response dispatch
    └── Formatted message back through connector-telegram
```

**Bot also runs rules:** The rule engine from `springtale-core` runs inside the bot
runtime. Cron rules, webhook rules, filesystem watch rules — they all fire through
the same event loop. A chat command and a cron trigger go through the same dispatch
path. The bot is the runtime; rules are one thing it handles.

**Crates added in Phase 1b (two milestones):**

**1b-i: Bot core** — `springtale-bot` ships with command router, handler dispatch,
state management, persona config, persistent memory. Testable against existing
Phase 1a connectors (connector-http webhooks, connector-filesystem events, etc.)
without any chat platform. The bot runs headless, processing rules and responding
to triggers. This validates the core architecture independently.

**1b-ii: First chat connector** — `connector-telegram` ships separately. Telegram
Bot API client (typed, async, webhook + polling modes). Once installed, the bot
becomes reachable via Telegram chat. The command router maps Telegram messages
to bot handlers. This is where users can actually switch from OpenClaw — they
talk to the bot through Telegram and get the same functionality, safely.

### Phase 2a — OpenClaw Feature Parity

**Goal:** Full chat platform coverage. AI adapters as fallback parser and optional
pipeline action. NL→Rule parser. Runtime behavioral monitoring. Sub-agent orchestration.
This is where Springtale becomes a full, safe replacement for OpenClaw.

**Transport:** `HttpTransport` (LAN/VPN, axum server + reqwest client, mTLS)
**AI:** User brings their own: Ollama, any OpenAI-compatible API, Anthropic (all optional, NoopAdapter default)
**AI roles:** (1) Fallback parser — when no command matches, user's AI interprets freeform input.
(2) NL→Rule — "notify me when X" → user's AI generates TOML rule, runs deterministically forever.
(3) `AiComplete` pipeline action — user's AI as one tool in workflows (summarize, analyze).
All three are optional. All degrade gracefully with `NoopAdapter`. We don't provide AI. We provide the socket.

**Why this obsoletes OpenClaw (310K+ GitHub stars, Node.js):**
OpenClaw is a Node.js gateway that runs skills with full machine access —
no sandboxing, no manifest signing, no capability declarations. Cisco's AI
security team demonstrated data exfiltration through third-party skills.
People need a drop-in alternative that does the same things but doesn't leave
them exposed. Free. Open source. No catches.

**Connectors shipping in Phase 2a:**
- `connector-discord` (gateway WebSocket, slash commands, embeds)
- `connector-signal` (signal-cli bridge, E2E encrypted channel)
- `connector-whatsapp` (sandboxed Baileys-equivalent, QR pairing)
- `connector-matrix` (Matrix SDK, federated rooms, E2E encryption)
- `connector-irc` (lightweight, raw TCP + TLS)
- `connector-slack` (Socket mode, slash commands, blocks, threads)
- `connector-nostr` (NIP-01 relay, event signing, encrypted DMs)

**Also shipping in Phase 2a:**
- AI adapters: `OpenAiCompatAdapter`, `AnthropicAdapter`, voice STT/TTS
- NL→Rule parser (`springtale-ai::parser`) — "remind me at 5pm" → TOML rule
- `springtale-sentinel` — runtime behavioral monitor
- Recursive pipeline orchestration (Clicky-derived subagent pattern)
- ATProto bot patterns (malwarevangelist-bot-derived)
- Heartbeat monitor, context compaction
- Multimedia pipeline (typed `Attachment`, image/audio/document handling)

### Phase 2b — Desktop + Mobile

**Goal:** Tauri 2 desktop and mobile shell. Visual rule builder. Web dashboard.
Browser automation.

**Delivery:** Tauri app for macOS, Windows, Linux, iOS, Android + `springtale-dashboard` web UI

**What ships:**
- Tauri 2 shell — same SolidJS + Rust codebase, five platform targets
- Visual rule builder — generates TOML, same format as hand-authored rules
- Canvas/A2UI — live UI surface the bot can push content to
- `springtale-dashboard` — web control UI served by `springtaled`
- `connector-browser` (headless Chromium, sandboxed, domain allow-list)
- Mobile: Swift (iOS) + Kotlin (Android) plugins — camera, NFC, biometric, push, voice
- Device pairing via QR code or Bonjour/mDNS discovery

**Mobile AI — bring your own, four options:**

On desktop, the user runs Ollama locally and the bot connects to `localhost:11434`.
On mobile, you can't run Ollama on your phone. The mobile app supports four
ways to bring your own AI, from most private to most convenient:

| Option | How it works | Privacy | Latency |
|---|---|---|---|
| **Connect to home server** | Mobile app pairs with user's `springtaled` instance (QR code or mDNS discovery). Bot runs on the server with local Ollama. Phone is a thin client. | **Maximum** — data never leaves user's network | LAN: ~10ms. VPN/Tailscale: ~50ms |
| **On-device model** | llama.cpp via Tauri native plugin. Small model (3-7B) runs directly on phone. | **Maximum** — data never leaves device | ~1-5s per response (device-dependent) |
| **User's API key** | User configures their OpenAI/Anthropic/etc key on the mobile app. Same as desktop remote provider. | **User's choice** — data goes to their chosen provider | ~1-2s |
| **No AI** | `NoopAdapter`. Bot works for commands, rules, automations. No freeform conversation. | **Maximum** — no data sent anywhere | Instant |

The default is **no AI** (`NoopAdapter`). The pairing flow (option 1) is the
recommended path — it gives the user their full home Ollama setup on their phone
without any data leaving their network. The QR code / mDNS discovery flow is
already needed for device pairing (connectors, rules sync), so AI comes along
for free once the phone knows where `springtaled` lives.

### Phase 3 — Veilid Mesh

**Goal:** Swap `HttpTransport` for `VeilidTransport` when `rekindle-protocol` is
production-stable. Agents discover peers via DHT. Connector registry becomes
distributed. No central coordination server. Bots become headless Veilid
community members (Rekindle §31 APAS pattern).

**Transport:** `VeilidTransport` (wraps `rekindle-protocol` crate)
**Delivery:** One config change, one new impl file. Zero business logic changes.

**Rekindle integration points:**
- `VeilidTransport` imports `rekindle_protocol::VeilidNode` for DHT and
  private route message delivery (three-path model: SMPL write, gossip, watch).
- Agent bots join Rekindle communities as headless member nodes, using the same
  SMPL slot_seed derivation and Ed25519 identity as human members.
- Connector registry migrates from PostgreSQL to Veilid DHT records, using the
  same universal SMPL schema (o_cnt: 0, 255 member subkeys) that Rekindle
  uses for governance and channel records.
- Bot SDK wraps `rekindle-protocol` into `springtale-bot` API: `on_message`,
  `send`, `on_governance_change`, `on_member_join` (Rekindle §31).
- HKDF pseudonyms provide cross-community identity unlinkability for bots,
  matching the privacy model for human users.

---

## 4. Workspace Layout

```
springtale/
├── Cargo.toml                        # [workspace] root
├── Cargo.lock
├── .cargo/
│   └── config.toml                   # target, rustflags, banned crates
├── flake.nix                         # Konductor-based dev shell (mirrors Rekindle)
├── flake.lock
├── .envrc                            # direnv: use flake
│
├── crates/                           # Pure Rust library crates (no Tauri dependency)
│   ├── springtale-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pipeline/             # Stage composition, retry logic
│   │       │   ├── mod.rs
│   │       │   ├── context.rs        # PipelineContext<I,O> + Attachment
│   │       │   ├── stage.rs          # Stage trait
│   │       │   ├── compose.rs        # compose_pipeline(), retry graph
│   │       │   └── error.rs          # PipelineError
│   │       ├── rule/
│   │       │   ├── mod.rs
│   │       │   ├── types.rs          # Rule, RuleId, RuleVersion, RuleStatus
│   │       │   ├── trigger.rs        # Trigger enum (Cron, FileWatch, Webhook, etc.)
│   │       │   ├── condition.rs      # Condition enum (And/Or/Not/Field/Time/Regex)
│   │       │   ├── action.rs         # Action enum (RunConnector, Chain, Transform, etc.)
│   │       │   ├── engine.rs         # RuleEngine: load, match, dispatch — NO AI dependency
│   │       │   ├── evaluate.rs       # ConditionEvaluator: Condition × Payload → bool
│   │       │   ├── template.rs       # RuleTemplate: pre-built IFTTT-style recipes
│   │       │   └── parse.rs          # TOML rule file parser + validator
│   │       ├── transform/            # Data transformation stages (non-AI)
│   │       │   ├── mod.rs
│   │       │   ├── extract.rs        # JSONPath / field extraction
│   │       │   ├── format.rs         # String formatting, template resolution
│   │       │   └── filter.rs         # Filter/map/reduce on collections
│   │       └── router/
│   │           ├── mod.rs
│   │           └── dispatch.rs       # Routes triggers to RuleEngine
│   │
│   ├── springtale-crypto/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── identity/
│   │       │   ├── mod.rs
│   │       │   ├── keypair.rs        # Ed25519 keypair generation + persistence
│   │       │   └── node_id.rs        # NodeId newtype over [u8; 32]
│   │       ├── vault/
│   │       │   ├── mod.rs
│   │       │   ├── store.rs          # Encrypted key-value store (XChaCha20-Poly1305)
│   │       │   └── kdf.rs            # Argon2id key derivation
│   │       ├── signature/
│   │       │   ├── mod.rs
│   │       │   ├── sign.rs           # Sign arbitrary bytes with Ed25519
│   │       │   └── verify.rs         # Verify signatures, canonical JSON
│   │       └── token/
│   │           ├── mod.rs
│   │           └── capability_token.rs  # Signed capability grant tokens
│   │
│   ├── springtale-transport/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── transport/
│   │       │   ├── mod.rs
│   │       │   └── trait_.rs         # Transport trait (send/recv/node_id/name)
│   │       ├── local/
│   │       │   ├── mod.rs
│   │       │   └── unix_socket.rs    # Phase 1: tokio UnixListener
│   │       ├── http/
│   │       │   ├── mod.rs
│   │       │   ├── server.rs         # Phase 2: axum inbound
│   │       │   └── client.rs         # Phase 2: reqwest outbound, mTLS
│   │       └── veilid/
│   │           ├── mod.rs
│   │           ├── stub.rs           # Phase 3 stub: VeilidTransport (unimplemented!())
│   │           ├── node.rs           # Phase 3: wraps rekindle_protocol::VeilidNode
│   │           ├── dht.rs            # Phase 3: DHT record read/write for registry
│   │           ├── gossip.rs         # Phase 3: gossip broadcast via app_message
│   │           └── routes.rs         # Phase 3: private route management
│   │
│   ├── springtale-connector/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manifest/
│   │       │   ├── mod.rs
│   │       │   ├── types.rs          # ConnectorManifest, Capability enum
│   │       │   └── verify.rs         # verify_signature(), verify_capabilities()
│   │       ├── sandbox/
│   │       │   ├── mod.rs
│   │       │   ├── runtime.rs        # Wasmtime Engine + Store per connector
│   │       │   ├── limits.rs         # Fuel metering, memory limits
│   │       │   └── host_api.rs       # WASI host functions exposed to guest
│   │       ├── registry/
│   │       │   ├── mod.rs
│   │       │   ├── store.rs          # In-memory + persisted connector registry
│   │       │   └── loader.rs         # Load, verify, instantiate connectors
│   │       └── capability/
│   │           ├── mod.rs
│   │           └── grant.rs          # CapabilityGrant, runtime enforcement
│   │
│   ├── springtale-scheduler/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── cron/
│   │       │   ├── mod.rs
│   │       │   └── executor.rs       # tokio cron job runner
│   │       ├── watcher/
│   │       │   ├── mod.rs
│   │       │   └── fs_watcher.rs     # notify-based filesystem event watcher
│   │       ├── queue/
│   │       │   ├── mod.rs
│   │       │   ├── producer.rs       # enqueue jobs via StorageBackend trait
│   │       │   └── consumer.rs       # Blocking pop, concurrency limit
│   │       ├── heartbeat/            # Phase 2: proactive wake cycle
│   │       │   ├── mod.rs
│   │       │   ├── monitor.rs        # Periodic wake (configurable, default 30min)
│   │       │   └── checklist.rs      # Rule-based condition evaluation
│   │       └── retry/
│   │           ├── mod.rs
│   │           └── backoff.rs        # Exponential backoff with jitter
│   │
│   ├── springtale-store/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── backend/
│   │       │   ├── mod.rs
│   │       │   └── trait_.rs         # StorageBackend trait (SQLite default, PostgreSQL optional)
│   │       ├── schema/
│   │       │   ├── mod.rs
│   │       │   ├── connectors.rs     # connectors table type
│   │       │   ├── events.rs         # events table type
│   │       │   ├── rules.rs          # rules table type
│   │       │   └── jobs.rs           # scheduler jobs table type
│   │       ├── schema/sql/         # Declarative DDL, one file per domain
│   │       │   ├── connectors.sql
│   │       │   ├── rules.sql
│   │       │   ├── events.sql
│   │       │   └── …               # bot.sql, audit.sql, safety.sql, formations.sql, etc.
│   │       └── queries/
│   │           ├── mod.rs
│   │           ├── connectors.rs     # CRUD for connector registry
│   │           ├── events.rs         # Event log queries
│   │           ├── rules.rs          # Rule management queries
│   │           └── jobs.rs           # Job queue persistence
│   │
│   ├── springtale-ai/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── adapter/
│   │       │   ├── mod.rs
│   │       │   └── trait_.rs         # AiAdapter trait (complete/stream/parse_rule/is_available)
│   │       ├── parser/               # NL→Rule: the core AI authoring convenience
│   │       │   ├── mod.rs
│   │       │   ├── rule_gen.rs       # NL intent → structured Rule (TOML-serializable)
│   │       │   └── prompt.rs         # Prompt templates, connector schema injection
│   │       ├── noop/
│   │       │   ├── mod.rs
│   │       │   └── adapter.rs        # NoopAdapter — default, zero dependencies
│   │       ├── ollama/
│   │       │   ├── mod.rs
│   │       │   ├── adapter.rs        # OllamaAdapter impl
│   │       │   ├── client.rs         # HTTP client for Ollama REST API
│   │       │   └── types.rs          # Request/response types
│   │       ├── openai/               # Phase 2: OpenAI-compatible API adapter
│   │       │   ├── mod.rs
│   │       │   ├── adapter.rs        # OpenAiCompatAdapter (GPT, Gemini, Kimi, OpenRouter)
│   │       │   └── client.rs         # /v1/chat/completions reqwest client
│   │       ├── anthropic/            # Phase 2: Claude API adapter
│   │       │   ├── mod.rs
│   │       │   ├── adapter.rs        # AnthropicAdapter with native tool_use
│   │       │   └── client.rs         # Claude /v1/messages reqwest client
│   │       └── voice/                # Phase 2: STT + TTS
│   │           ├── mod.rs
│   │           ├── stt.rs            # Whisper-compatible speech-to-text
│   │           └── tts.rs            # ElevenLabs / Piper text-to-speech
│   │
│   ├── springtale-mcp/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── server/
│           │   ├── mod.rs
│           │   └── builder.rs        # Build rmcp McpServer from Connector
│           ├── adapter/
│           │   ├── mod.rs
│           │   └── connector.rs      # Adapt any Connector → MCP tool list
│           └── transport/
│               ├── mod.rs
│               ├── stdio.rs          # StdioServerTransport wrapper
│               └── sse.rs            # SSE transport for remote MCP
│
│   └── springtale-bot/                    # Phase 1b: classical bot runtime
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── runtime/
│   │       │   ├── mod.rs
│   │       │   ├── event_loop.rs     # Main bot event loop — receives from all connectors
│   │       │   ├── headless.rs       # Headless runtime (no Tauri, no GUI)
│   │       │   └── lifecycle.rs      # Start, health check, graceful shutdown
│   │       ├── router/               # Classical command routing (no AI)
│   │       │   ├── mod.rs
│   │       │   ├── prefix.rs         # /search, /help, /remind prefix commands
│   │       │   ├── pattern.rs        # Regex/keyword matching
│   │       │   ├── alias.rs          # User-defined command aliases
│   │       │   └── fallback.rs       # No match → unknown cmd (1b) or AI (2a)
│   │       ├── handler/              # Command handler dispatch
│   │       │   ├── mod.rs
│   │       │   ├── registry.rs       # Handler registration + dispatch
│   │       │   ├── builtin.rs        # /help, /status, /rules, /connectors
│   │       │   └── connector.rs      # Route command to named connector action
│   │       ├── state/                # Conversation state
│   │       │   ├── mod.rs
│   │       │   ├── session.rs        # Per-user, per-channel session tracking
│   │       │   ├── prefs.rs          # User preferences
│   │       │   └── persona.rs        # Bot persona config
│   │       ├── identity/
│   │       │   ├── mod.rs
│   │       │   └── bot_id.rs         # BotId: Ed25519 + HKDF pseudonym
│   │       ├── memory/
│   │       │   ├── mod.rs
│   │       │   ├── persistent.rs     # SQLite-backed persistent memory
│   │       │   ├── context.rs        # Conversation context window
│   │       │   └── compaction.rs     # Truncate (1b) or AI summarize (2a)
│   │       ├── orchestrator/         # Phase 2a: recursive sub-agents
│   │       │   ├── mod.rs
│   │       │   ├── recursive.rs      # Recursive pipeline spawning (Clicky pattern)
│   │       │   ├── subagent.rs       # Spawn child with scoped capabilities
│   │       │   └── coordinator.rs    # Multi-bot coordination
│   │       └── bridge/               # Phase 2a+: platform bridges
│   │           ├── mod.rs
│   │           ├── rekindle.rs       # Phase 3: Rekindle community member bridge
│   │           └── atproto.rs        # ATProto bot bridge (malwarevangelist)
│
│   └── springtale-sentinel/               # Phase 2a: runtime behavioral monitor
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── rate_limiter.rs       # Actions/minute per connector
│           ├── circuit_breaker.rs    # Per-stage failure counting, auto-disable
│           ├── dead_man.rs           # Actions/minute without user interaction
│           ├── toxic_pairs.rs        # Dangerous capability combinations
│           ├── impact.rs             # Action impact: read-only/reversible/destructive
│           └── audit/
│               ├── trail.rs          # Append-only audit trail (SQLite)
│               └── export.rs         # Export for CLI review
│               └── export.rs        # Compliance export
│
├── connectors/                       # First-party connectors (pure Rust)
│   ├── connector-kick/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs             # KickConfig with Secret<String> fields
│   │       ├── auth/
│   │       │   ├── mod.rs
│   │       │   ├── oauth.rs          # OAuth2 PKCE flow
│   │       │   └── token.rs          # Token storage, refresh logic
│   │       ├── client/
│   │       │   ├── mod.rs
│   │       │   └── api.rs            # Typed Kick API client
│   │       ├── triggers/
│   │       │   ├── mod.rs
│   │       │   ├── chat_message.rs
│   │       │   └── stream_live.rs
│   │       └── actions/
│   │           ├── mod.rs
│   │           ├── chat_send.rs
│   │           └── channel_get.rs
│   │
│   ├── connector-presearch/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── client/
│   │       │   ├── mod.rs
│   │       │   └── api.rs            # Presearch REST client
│   │       ├── cache/
│   │       │   ├── mod.rs
│   │       │   └── result_cache.rs   # TTL cache for search results
│   │       └── actions/
│   │           ├── mod.rs
│   │           ├── search.rs
│   │           └── scrape.rs
│   │
│   ├── connector-bluesky/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── atproto/
│   │       │   ├── mod.rs
│   │       │   ├── session.rs        # ATP session management
│   │       │   └── lexicon.rs        # app.bsky.* lexicon types
│   │       ├── firehose/
│   │       │   ├── mod.rs
│   │       │   └── jetstream.rs      # Jetstream WebSocket consumer
│   │       ├── triggers/
│   │       │   ├── mod.rs
│   │       │   ├── mention.rs
│   │       │   └── follow.rs
│   │       └── actions/
│   │           ├── mod.rs
│   │           ├── post.rs
│   │           └── reply.rs
│   │
│   ├── connector-github/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── client/
│   │       │   └── api.rs            # GitHub REST + GraphQL
│   │       ├── webhook/
│   │       │   ├── mod.rs
│   │       │   └── verify.rs         # HMAC-SHA256 signature verification
│   │       ├── triggers/
│   │       │   ├── mod.rs
│   │       │   ├── push.rs
│   │       │   ├── pull_request.rs
│   │       │   └── issue.rs
│   │       └── actions/
│   │           ├── mod.rs
│   │           ├── create_issue.rs
│   │           └── comment.rs
│   │
│   ├── connector-filesystem/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── watcher/
│   │       │   └── notify.rs         # notify crate wrapper
│   │       └── actions/
│   │           ├── mod.rs
│   │           ├── read.rs
│   │           ├── write.rs
│   │           └── list.rs
│   │
│   ├── connector-shell/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── sandbox/
│   │       │   ├── mod.rs
│   │       │   └── policy.rs         # Allow-list of permitted commands
│   │       └── actions/
│   │           ├── mod.rs
│   │           └── exec.rs           # tokio::process::Command, timeout enforced
│   │
│   └── connector-http/
│       └── src/
│           ├── lib.rs
│           ├── config.rs
│           ├── client/
│           │   └── request.rs        # reqwest wrapper, rustls-tls
│           └── actions/
│               ├── mod.rs
│               ├── get.rs
│               ├── post.rs
│               └── webhook_listen.rs
│
│   # ── Phase 1b/2a Chat + Service Connectors (all shipped) ──
│   connector-telegram/              # Phase 1b: Bot API typed client, polling + webhook
│   connector-discord/               # Phase 2a: Gateway WebSocket via twilight, slash commands
│   connector-signal/                # Phase 2a: signal-cli bridge, E2E encrypted
│   connector-irc/                   # Phase 2a: Raw TCP + TLS, lightweight
│   connector-slack/                 # Phase 2a: Socket mode, slash commands, blocks
│   connector-browser/               # Phase 2a: Headless Chromium, domain allow-list
│   connector-nostr/                 # Phase 2a: NIP-01 relay, event signing, NIP-44 DMs
│   # connector-matrix/                # Deferred: matrix-sdk pins CVE-bearing rusqlite 0.37
│   # connector-whatsapp/              # Removed from roadmap (Meta terms incompatible with threat model)
│   # connector-rekindle/              # Removed: Phase 3 P2P concern, lives in Rekindle's own repo
│
├── apps/
│   ├── springtaled/                       # Headless daemon
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config/
│   │       │   ├── mod.rs
│   │       │   └── load.rs           # figment layered config (TOML + env) with Secret<T>
│   │       ├── server/
│   │       │   ├── mod.rs
│   │       │   └── http.rs           # axum management API
│   │       └── runtime/
│   │           ├── mod.rs
│   │           └── boot.rs           # Init order: store→crypto→transport→connectors
│   │
│   └── springtale-cli/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── commands/
│           │   ├── mod.rs
│           │   ├── connector.rs      # connector install/list/remove
│           │   ├── rule.rs           # rule add/list/toggle
│           │   └── run.rs            # run a rule manually
│           └── output/
│               ├── mod.rs
│               └── format.rs         # Table + JSON output modes
│
├── tauri/                            # Desktop shell (Phase 2)
│   ├── src-tauri/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── commands/
│   │       │   ├── mod.rs
│   │       │   ├── connectors.rs     # Tauri commands wrapping springtale-connector
│   │       │   ├── rules.rs
│   │       │   ├── scheduler.rs
│   │       │   └── ai.rs
│   │       ├── channels/
│   │       │   ├── mod.rs
│   │       │   └── events.rs         # Event type definitions for frontend
│   │       └── services/
│   │           ├── mod.rs
│   │           ├── runtime.rs        # Manages agent runtime lifecycle
│   │           └── tray.rs           # System tray integration
│   └── src/                          # SolidJS frontend (TypeScript, rendering only)
│       ├── windows/
│       ├── components/
│       ├── stores/
│       ├── ipc/                      # Typed invoke() wrappers only
│       └── styles/
│
└── sdk/
    └── connector-sdk-ts/             # TypeScript SDK for community connectors
        ├── src/
        │   ├── connector.ts          # BaseConnector class + types
        │   ├── manifest.ts           # ConnectorManifest schema (Zod v4)
        │   ├── capability.ts         # Capability enum (mirrors Rust)
        │   └── host/
        │       └── api.ts            # Host function bindings (WASI imports)
        ├── package.json
        └── tsconfig.json             # target: wasm32-wasi via @bytecodealliance/jco
```

---

## 5. Cargo Workspace Dependencies

All version pins live at workspace root. No crate specifies its own version for
shared dependencies. This is the 2026 Rust standard for monorepos.

```toml
# Cargo.toml (workspace root) — reflects current shipped reality (2026-05).
# The original spec listed Phase 2 connectors as commented-out stubs;
# they have all shipped since.

[workspace]
resolver = "2"
members  = [
  # Phase 1a foundation
  "crates/springtale-core",
  "crates/springtale-crypto",
  "crates/springtale-transport",
  "crates/springtale-connector",
  "crates/springtale-scheduler",
  "crates/springtale-store",
  "crates/springtale-ai",
  "crates/springtale-mcp",
  "crates/springtale-runtime",
  # Phase 1b bot runtime + cooperation framework (extracted April 2026)
  "crates/springtale-bot",
  "crates/springtale-cooperation",           # 42 pub modules, zero internal deps
  # Phase 2a sentinel
  "crates/springtale-sentinel",
  # G3 cross-language bindings (May 2026)
  "crates/springtale-wit",                   # WIT world for WASM Component Model
  "crates/springtale-py",                    # pyo3 Python bindings (cdylib + rlib)
  # Vendored SQLite3MultipleCiphers shim
  "crates/libsqlite3-sys-mc",
  # Phase 1a baseline connectors (7)
  "connectors/connector-kick",
  "connectors/connector-presearch",
  "connectors/connector-bluesky",
  "connectors/connector-github",
  "connectors/connector-filesystem",
  "connectors/connector-shell",
  "connectors/connector-http",
  # Phase 1b chat connector
  "connectors/connector-telegram",
  # Phase 2a connectors (all shipped — were commented out as stubs in spec v2.0)
  "connectors/connector-discord",
  "connectors/connector-signal",
  "connectors/connector-irc",
  "connectors/connector-slack",
  "connectors/connector-nostr",
  "connectors/connector-browser",
  # Deferred — `matrix-sdk` pins rusqlite 0.37 with an open CVE.
  # Excluded from workspace via exclude = [...] below.  Will ship once
  # upstream catches up to a patched rusqlite.
  # "connectors/connector-matrix",
  # Removed from intent — `connector-whatsapp` and `connector-rekindle`
  # appeared in spec v2.0 as planned Phase 2 connectors but are no
  # longer on the roadmap:
  #   - WhatsApp:  Meta's API terms are incompatible with the threat
  #     model (no anonymity, account binding to phone, ban risk for
  #     automation).  Not shipping.
  #   - Rekindle:  Phase 3 P2P concern; lives in Rekindle's own repo
  #     when Veilid stabilises.
  "apps/springtaled",
  "apps/springtale-cli",
]

[workspace.dependencies]

# ── Async Runtime ──────────────────────────────────────────────────────────────
tokio            = { version = "1",    features = ["full"] }  # LTS 1.47.x until Sep 2026
tokio-util       = { version = "0.7",  features = ["codec"] }
async-trait      = "0.1"

# ── Observability ──────────────────────────────────────────────────────────────
tracing          = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# ── Error Handling ─────────────────────────────────────────────────────────────
anyhow           = "1"           # application-level error propagation
thiserror        = "2"           # library-level error types

# ── Serialization ──────────────────────────────────────────────────────────────
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"
toml             = "0.8"
schemars         = { version = "1",   features = ["derive"] }  # JSON Schema gen (v1 stable 2025)

# ── Validation ─────────────────────────────────────────────────────────────────
garde            = { version = "0.22", features = ["derive", "full"] }

# ── Security / Secrets ─────────────────────────────────────────────────────────
secrecy          = { version = "0.10", features = ["serde"] }
zeroize          = { version = "1",   features = ["derive"] }

# ── Cryptography ───────────────────────────────────────────────────────────────
ed25519-dalek    = { version = "2",   features = ["rand_core", "serde"] }
chacha20poly1305 = "0.10"
argon2           = "0.5"
rand             = { version = "0.9", features = ["getrandom"] }
sha2             = "0.10"
hmac             = "0.12"

# ── Identifiers ────────────────────────────────────────────────────────────────
uuid             = { version = "1",   features = ["v4", "serde"] }

# ── HTTP Server ────────────────────────────────────────────────────────────────
axum             = { version = "0.8", features = ["macros", "multipart"] }
tower            = "0.5"
tower-http       = { version = "0.6", features = ["cors", "trace", "auth", "limit"] }

# ── HTTP Client ────────────────────────────────────────────────────────────────
reqwest          = { version = "0.13", features = ["json", "rustls-tls", "stream"] }  # aws-lc-rs backend (rustls default)
# native-tls is banned — see .cargo/config.toml

# ── Database (local-first) ─────────────────────────────────────────────────────
rusqlite         = { version = "0.32", features = ["bundled", "vtab"] }  # default: SQLite, zero external deps

# ── Database (server mode, optional) ──────────────────────────────────────────
sqlx             = { version = ">=0.8.3",  features = ["postgres", "uuid", "chrono", "json", "migrate", "runtime-tokio"] }
# Only used with --features postgres. Phase 1 does NOT require PostgreSQL.
# No Redis. Job queue is a SQLite table polled by tokio tasks.

# ── WASM Sandbox ───────────────────────────────────────────────────────────────
wasmtime         = { version = ">=41.0.4",   features = ["cranelift", "component-model", "parallel-compilation"] }
wasmtime-wasi    = { version = ">=41.0.4",   features = ["component-model"] }

# ── MCP Protocol ───────────────────────────────────────────────────────────────
rmcp             = { version = "0.2",  features = ["server", "client", "transport-io"] }

# ── Filesystem Watching ────────────────────────────────────────────────────────
notify           = { version = "7",    features = ["macos_fsevent"] }

# ── Scheduling ─────────────────────────────────────────────────────────────────
cron             = "0.13"
chrono           = { version = "0.4",  features = ["serde"] }

# ── CLI ────────────────────────────────────────────────────────────────────────
clap             = { version = "4",    features = ["derive", "env"] }
indicatif        = "0.17"             # progress bars
tabled           = "0.17"             # table output

# ── Config ─────────────────────────────────────────────────────────────────────
figment          = { version = "0.10", features = ["toml", "env"] }  # layered config: TOML + env vars

# ── Concurrency ────────────────────────────────────────────────────────────────
dashmap          = "6"                # concurrent HashMap for sentinel behavioral profiles

# ── SQLite (bot memory) ───────────────────────────────────────────────────────

# ── Internal Crates ────────────────────────────────────────────────────────────
springtale-core       = { path = "crates/springtale-core" }
springtale-crypto     = { path = "crates/springtale-crypto" }
springtale-transport  = { path = "crates/springtale-transport" }
springtale-connector  = { path = "crates/springtale-connector" }
springtale-scheduler  = { path = "crates/springtale-scheduler" }
springtale-store      = { path = "crates/springtale-store" }
springtale-ai         = { path = "crates/springtale-ai" }
springtale-mcp        = { path = "crates/springtale-mcp" }
springtale-bot        = { path = "crates/springtale-bot" }
springtale-sentinel   = { path = "crates/springtale-sentinel" }
```

```toml
# .cargo/config.toml

[build]
rustflags = [
  "-D", "warnings",            # all warnings are errors
  "-D", "clippy::unwrap_used", # no unwrap() in library code
  "-D", "clippy::expect_used", # no expect() in library code (use thiserror)
  "-D", "clippy::panic",       # no panic!() in library code
]

# Ban native-tls to enforce rustls across all transitive dependencies
[patch.crates-io]
native-tls = { path = "vendor/native-tls-stub" }  # stub that panics at link time
```

---

## 6. Core Crates

### 6.1 `springtale-core`

**Purpose:** Pipeline composition engine, rule evaluation engine, and rule type
system. This is the heart of Springtale. Zero network, zero crypto, zero AI.
Pure business logic, independently testable. **Everything in this crate works
with `NoopAdapter` — AI is never required.**

**Key modules:**

| Module | Responsibility |
|---|---|
| `pipeline::context` | `PipelineContext<I,O>` — trace ID, input, output, errors, retry count, `Attachment` support for multimedia |
| `pipeline::stage` | `Stage` trait — `async fn call(ctx) -> Result<ctx>` |
| `pipeline::compose` | `compose_pipeline()` — left-to-right stage composition with Clicky-style retry graph |
| `pipeline::error` | `PipelineError` — wraps stage index + source error |
| `rule::types` | `Rule`, `RuleId`, `RuleVersion`, `RuleStatus` (enabled/disabled/draft) |
| `rule::trigger` | `Trigger` enum — Cron, FileWatch, Webhook, P2PMessage, SystemEvent, ConnectorEvent |
| `rule::condition` | `Condition` enum — FieldEquals, Contains, Regex, TimeInRange, DayOfWeek, And, Or, Not |
| `rule::action` | `Action` enum — RunConnector, SendMessage, WriteFile, RunShell, Notify, Chain, Transform, Delay |
| `rule::engine` | `RuleEngine` — loads rules, matches triggers to conditions, dispatches actions. **No AI dependency.** |
| `rule::evaluate` | `ConditionEvaluator` — evaluates condition trees against trigger payloads. Pure function, no side effects. |
| `rule::template` | `RuleTemplate` — pre-built rule patterns (IFTTT-style recipes). Users customize parameters, not logic. |
| `router::dispatch` | Routes incoming trigger events to `RuleEngine`, which matches and dispatches |
| `transform::pipeline` | Data transformation functions usable as pipeline stages — map, filter, extract, format, join, split |

**`Rule` definition (TOML format, not Markdown):**

```toml
# rules/kick-to-discord.toml — NO AI required

[rule]
name = "kick-stream-announce"
description = "When Kick stream goes live, announce on Discord and Bluesky"
enabled = true

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

[[conditions]]
type = "FieldEquals"
field = "category"
value = "gaming"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"
channel = "${DISCORD_ANNOUNCEMENTS}"
message = "🔴 ${trigger.username} is live on Kick: ${trigger.title}"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"
text = "${trigger.username} is streaming: ${trigger.title} https://kick.com/${trigger.username}"
```

**Non-AI workflow examples (proving the mission):**

```toml
# rules/download-organizer.toml — file automation, zero AI
[rule]
name = "organize-downloads"
[trigger]
type = "FileWatch"
path = "~/Downloads"
event = "create"
[[conditions]]
type = "Regex"
field = "filename"
pattern = "\\.(pdf|docx|xlsx)$"
[[actions]]
type = "WriteFile"
destination = "~/Documents/${trigger.extension}/${trigger.filename}"
delete_source = true

# rules/github-lint-gate.toml — CI automation, zero AI
[rule]
name = "pr-lint-check"
[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request_opened"
[[actions]]
type = "Chain"
steps = [
  { type = "RunConnector", connector = "connector-shell", command = "cargo clippy --workspace 2>&1" },
  { type = "RunConnector", connector = "connector-github", action = "post_comment", body = "Lint results:\n${prev.stdout}" },
]

# rules/rss-digest.toml — scheduled digest, zero AI
[rule]
name = "morning-rss-digest"
[trigger]
type = "Cron"
expression = "0 7 * * *"
[[actions]]
type = "Chain"
steps = [
  { type = "RunConnector", connector = "connector-http", url = "https://feeds.example.com/rss.xml" },
  { type = "Transform", operation = "extract", path = "items[0:5].title" },
  { type = "RunConnector", connector = "connector-telegram", action = "send_message", text = "Morning digest:\n${prev.output}" },
]
```

**When AI IS used, it slots in as an optional stage:**

```toml
# rules/pr-review-with-ai.toml — same as above, but AI enhances one stage
[rule]
name = "pr-ai-review"
[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request_opened"
[[actions]]
type = "Chain"
steps = [
  { type = "RunConnector", connector = "connector-github", action = "get_diff" },
  { type = "AiComplete", prompt = "Review this diff for security issues:\n${prev.output}", adapter = "ollama" },
  { type = "RunConnector", connector = "connector-github", action = "post_comment", body = "${prev.output}" },
]
# If AiComplete fails or NoopAdapter is configured, the chain skips to next
# non-AI step or reports the raw diff without analysis. Nothing breaks.
```

**Visual Rule Builder (Phase 2 Tauri UI):**

The visual rule builder is a SolidJS component in the Tauri shell that generates
TOML rule files. It is NOT a separate system — it produces the same TOML that
hand-authored rules use. No lock-in, no proprietary format.

```
┌─────────────────────────────────────────────────────────────┐
│ TRIGGER                                                     │
│ ┌─────────┐   ┌──────────────────┐   ┌────────────────┐    │
│ │ Cron ▾  │   │ connector-kick ▾ │   │ stream_live ▾  │    │
│ └─────────┘   └──────────────────┘   └────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│ CONDITIONS (optional)                                       │
│ ┌──────────┐   ┌─────────┐   ┌──────────┐                  │
│ │ field ▾  │   │ equals ▾│   │  gaming   │   [+ Add]       │
│ └──────────┘   └─────────┘   └──────────┘                  │
├─────────────────────────────────────────────────────────────┤
│ ACTIONS                                                     │
│ ① connector-discord → send_message → #announcements        │
│ ② connector-bluesky → create_post → ${trigger.title}       │
│ [+ Add Action]   [+ Add AI Step (optional)]                 │
├─────────────────────────────────────────────────────────────┤
│            [Test Rule]    [Save as TOML]    [Enable]        │
└─────────────────────────────────────────────────────────────┘
```

The builder enumerates available triggers from installed connectors, conditions
from the `Condition` enum, and actions from connector manifests. No hardcoded
lists — everything derives from the typed system.

**`RuleEngine` evaluation flow (no AI in the critical path):**

```
Trigger event arrives (from connector, cron, webhook, filesystem)
│
├── router::dispatch → find all Rules matching this trigger type
│
├── For each matching Rule:
│   ├── rule::evaluate → evaluate Condition tree against trigger payload
│   │   (pure function: Condition × Payload → bool)
│   │   No network. No AI. No side effects.
│   │
│   ├── If conditions pass:
│   │   ├── Build pipeline from Rule.actions
│   │   ├── Each action becomes a pipeline Stage
│   │   ├── AiComplete stages use configured adapter (or NoopAdapter)
│   │   ├── springtale-sentinel evaluates each stage before execution
│   │   └── Execute pipeline with fuel budget from autonomy level
│   │
│   └── If conditions fail: skip, log, continue to next rule
│
└── Emit RuleEvaluated event to event log
```

**Component security audit for `springtale-core`:**

| Concern | Control |
|---|---|
| Malicious rule injection | Rules loaded from TOML files on disk or springtale-store DB. Not from connector output. Not from AI output. User must explicitly create/import rules. |
| Condition evaluation DoS (deeply nested And/Or/Not) | Max condition depth: 8. Enforced in `ConditionEvaluator`. Deeper trees rejected at parse time. |
| Action chain amplification (Chain→Chain→Chain) | Max chain depth: 4 (matches pipeline recursive depth). Fuel budget applies to entire chain. |
| Context poisoning between rules | Each rule evaluation gets a fresh `PipelineContext`. No shared mutable state between concurrent rule evaluations. |
| Template injection via `${trigger.field}` | Template variables are sanitized: no nested `${}`, no code execution, no filesystem access. Variables resolve to string values only. |
| Regex DoS in Condition::Regex | Regex compilation uses `regex` crate with default size limit (10MB). Evaluation has 1-second timeout. |

**Component privacy audit for `springtale-core`:**

| Concern | Control |
|---|---|
| PII in pipeline context | `PipelineContext` is ephemeral — created per evaluation, dropped after. Not persisted unless the rule explicitly writes to store. |
| Trigger payloads contain user data | Trigger payloads are typed per connector. Connectors declare what data they collect in `DataDisclosure`. Payloads are not logged at DEBUG level by default. |
| Rule files may contain PII (usernames, channel IDs) | Rule files are local — never transmitted. Stored in `springtale-store` with same DB encryption as other data. |
| Template outputs may contain PII | Template resolution output is ephemeral. Logged at TRACE level only (disabled in production). |

**External dependencies:** `tokio`, `serde`, `schemars`, `garde`, `uuid`, `thiserror`, `tracing`, `regex`, `toml`

**No dependency on:** `springtale-crypto`, `springtale-transport`, `springtale-connector`, `springtale-ai`

---

### 6.2 `springtale-crypto`

**Purpose:** All cryptographic operations. Ed25519 identity, encrypted vault,
manifest signature verification, capability tokens.

**Key modules:**

| Module | Responsibility |
|---|---|
| `identity::keypair` | Generate, persist, and load Ed25519 signing keypairs |
| `identity::node_id` | `NodeId([u8; 32])` newtype — Veilid-compatible when Phase 3 arrives |
| `vault::store` | XChaCha20-Poly1305 encrypted key-value store for secrets at rest |
| `vault::kdf` | Argon2id password-based key derivation for vault unlock |
| `signature::sign` | Sign arbitrary bytes with Ed25519 signing key |
| `signature::verify` | Verify Ed25519 signature over canonical JSON |
| `token::capability_token` | Signed, expiring capability grants between agents |

**External dependencies:** `ed25519-dalek`, `chacha20poly1305`, `argon2`, `secrecy`,
`zeroize`, `rand`, `sha2`, `serde`, `thiserror`, `tracing`

**Security invariants:**
- `SigningKey` is always wrapped in `Secret<SigningKey>`
- `vault::store` zeroes plaintext buffers after encryption via `zeroize`
- No `derive(Debug)` on any type containing key material
- `canonical_json()` sorts keys deterministically before signing
- Nonce reuse prevention: XChaCha20 uses 192-bit nonces generated from OS CSPRNG (`rand::rngs::OsRng`). Collision probability negligible at 2^-96.
- Wrong passphrase timing: Argon2id is constant-time. Vault returns generic `VaultError::Locked`, no information leakage.
- Key rotation: `springtale-cli crypto rotate-vault-key` re-encrypts vault with new passphrase-derived key. Old key zeroed immediately.
- Vault file permissions: created with `0o600` (owner read/write only). Checked on load — warns if permissions are too open.

**Privacy audit:**
- Vault file location: `~/.local/share/springtale/vault.bin` (XDG standard). Not in a dotfile directory that infostealers typically target.
- Backup: vault file is encrypted at rest. Safe to back up without additional encryption. Passphrase never stored on disk.
- Key material never leaves process memory except through `expose_secret()` call sites (annotated, auditable).

---

### 6.3 `springtale-transport`

**Purpose:** Transport abstraction. All inter-node communication routes through
the `Transport` trait. Phase swaps are implementation-only.

**Key modules:**

| Module | Responsibility |
|---|---|
| `transport::trait_` | `Transport` trait — `send`, `recv`, `node_id`, `name` |
| `local::unix_socket` | Phase 1: tokio `UnixListener` + `UnixStream`, same-machine only |
| `http::server` | Phase 2: axum inbound endpoint, mTLS with `rustls` |
| `http::client` | Phase 2: reqwest outbound, peer cert validation, `rustls-tls` |
| `veilid::stub` | Phase 3 stub: `VeilidTransport` struct with `unimplemented!()` bodies |
| `veilid::node` | Phase 3: wraps `rekindle_protocol::VeilidNode`, manages attach/detach lifecycle |
| `veilid::dht` | Phase 3: DHT record read/write — `set_dht_value` / `get_dht_value` for registry |
| `veilid::gossip` | Phase 3: gossip broadcast via `app_message` to private routes (fire-and-forget) |
| `veilid::routes` | Phase 3: private route creation, import, refresh, expiry management |

All application code takes `Arc<dyn Transport + Send + Sync>`. No concrete
transport type escapes this module. Phase 3 replaces `veilid::stub` with
an import of `rekindle_protocol::VeilidNode`.

**Phase 3 `VeilidTransport` implementation plan (Rekindle three-path model):**

The `VeilidTransport` wraps Rekindle's three-path delivery model into the
`Transport` trait:

```
Transport::send(to, msg)
├── PATH 1: SMPL Write (primary, durable)
│   Write message to sender's own subkey in a shared SMPL record.
│   Retry with exponential backoff on failure.
│   Persists for offline catchup — receiver reads when they come online.
│
├── PATH 2: Gossip (secondary, low-latency)
│   import_remote_private_route(receiver.route_blob)
│   app_message(Target::RouteId(route_id), signed_bytes)
│   Fire-and-forget. Sub-second delivery to online peers.
│
└── PATH 3: Watch + Inspect (consistency backstop)
    Receiver calls watch_dht_values() on shared SMPL records.
    inspect_dht_record() every 60s for lightweight gap detection.
    Catches what gossip drops. Polling fallback for failed watches.

Transport::recv()
    Merge from all three paths → dedup by message_id
    → sort by Lamport timestamp → return next message.
```

Any single path succeeding is sufficient. This gives the same reliability
guarantees as the `HttpTransport` (confirmed delivery via mTLS) while
adding offline resilience (SMPL persistence) and privacy (private routes).

**Identity mapping:** `Transport::node_id()` returns the Veilid-format
`TypedKey` wrapped in `NodeId`. The Ed25519 keypair from `springtale-crypto`
is used directly as the Veilid identity — no separate key generation.

**NAT traversal:** Veilid's VICE layer handles NAT traversal transparently.
Direct → hole-punch → signal-reverse → relay. No application-level concern.

**External dependencies:** `tokio`, `axum`, `tower-http`, `reqwest`, `serde`,
`tracing`, `async-trait`, `springtale-crypto` (for `NodeId`)

**Security audit:**
- DNS rebinding: `HttpTransport` validates `Host` header against configured bind address. Rejects requests with mismatched host.
- Rate limiting: `tower-http::limit` applied at transport layer. Default: 100 req/s per peer. Configurable in `springtale.toml`.
- TLS cert pinning: Phase 2 mTLS uses peer certificate fingerprint list, not just CA validation. Fingerprints stored in `springtale-store`.
- Message size limit: `Transport::recv()` rejects messages > 16 MiB. Prevents memory exhaustion from malicious peers.
- Unix socket permissions: Phase 1 `LocalTransport` socket file created with `0o600`. Only the owning user can connect.

**Privacy audit:**
- Transport headers: `HttpTransport` strips all non-essential headers. No `User-Agent`, no `Server` header in responses.
- Connection timing: Phase 3 `VeilidTransport` uses privacy routes — sender/receiver IPs hidden from each other. Phase 2 mTLS reveals peer IP to each other (acceptable for LAN/VPN scope).
- Message metadata: `Message` struct contains only `id` and `payload`. No timestamps, no sender info in the envelope — that's handled by the payload layer.

---

### 6.4 `springtale-connector`

**Purpose:** Connector runtime. Manages loading, verification, lifecycle, and
execution of connectors. Two connector types with different trust models:

| Type | Trust | Isolation | Who writes them |
|---|---|---|---|
| `NativeConnector` | High — audited, signed by Springtale team | In-process, capability-checked at runtime | First-party Rust (connector-kick, connector-telegram, etc.) |
| `WasmConnector` | Low — community-authored, untrusted code | Wasmtime WASM sandbox, fuel metering, memory limits | Community via TypeScript SDK or Rust→WASM |

This is the same trust model as browser extensions vs web pages. Native connectors
are like browser built-in features — they run in the same process but with declared
permissions. WASM connectors are like web pages — they run in a sandbox and cannot
escape it.

**Key modules:**

| Module | Responsibility |
|---|---|
| `connector::trait_` | `Connector` trait — `triggers()`, `actions()`, `execute(action, input)`, `on_event(handler)` |
| `manifest::types` | `ConnectorManifest`, `Capability` enum, `TriggerDecl`, `ActionDecl`, `DataDisclosure` |
| `manifest::verify` | `verify_signature()`, `verify_capabilities()`, canonical JSON serialization |
| `native::runtime` | `NativeConnector` loader — loads first-party Rust connectors, checks declared capabilities at runtime |
| `native::capability` | Runtime capability checks — before every `execute()` call, validates the action is within declared scope |
| `wasm::runtime` | `WasmConnector` loader — Wasmtime `Engine` (shared, compiled once) + per-connector `Store` |
| `wasm::limits` | `SandboxLimits` — fuel units, memory pages, execution timeout |
| `wasm::host_api` | WASI host functions exposed to WASM guests — network (capability-gated), keychain read, logging |
| `registry::store` | In-memory + SQLite-persisted connector registry |
| `registry::loader` | Load manifest → verify signature → check capabilities → instantiate → register |

**How a connector loads (step by step):**

```
springtale connector install ./connector-kick.toml
│
├── 1. Parse manifest TOML → ConnectorManifest struct (garde validation)
├── 2. Verify Ed25519 signature against author pubkey in manifest
│      If invalid → reject, log, done
├── 3. Check capabilities against user policy
│      High-privilege caps (ShellExec, NetworkOutbound) → prompt user for approval
│      User rejects → connector not loaded
├── 4. For WasmConnector: verify WASM binary hash matches manifest.wasm_hash
│      Load into Wasmtime Engine with SandboxLimits applied
│      For NativeConnector: load Rust connector struct, register capability checker
├── 5. Register in springtale-store (SQLite)
├── 6. Connector available for use in rules and bot commands
```

**`Connector` trait (what every connector implements):**

```rust
// crates/springtale-connector/src/connector/trait_.rs

#[async_trait]
pub trait Connector: Send + Sync + 'static {
    /// What events this connector can emit (used by rule engine for trigger matching).
    fn triggers(&self) -> &[TriggerDecl];

    /// What actions this connector can perform (used by bot command routing + rule actions).
    fn actions(&self) -> &[ActionDecl];

    /// Execute an action. Input/output are typed JSON values validated against ActionDecl schema.
    async fn execute(&self, action: &str, input: serde_json::Value) -> Result<ActionResult>;

    /// Register an event handler for a trigger type. Called by the bot event loop.
    async fn on_event(&self, trigger: &str, handler: EventHandler) -> Result<()>;

    /// Connector metadata for registry and MCP tool generation.
    fn manifest(&self) -> &ConnectorManifest;
}
```

**WASM sandbox configuration (Wasmtime):**

```rust
// crates/springtale-connector/src/wasm/limits.rs

pub struct SandboxLimits {
    pub fuel: u64,              // instruction budget per invocation (default: 10_000_000)
    pub memory_pages: u32,      // WASM memory pages, 64KB each (default: 1024 = 64MB)
    pub timeout: Duration,      // wall-clock timeout (default: 30s)
    pub max_response_bytes: usize, // output size limit (default: 1MB)
}
```

The `Engine` is created once at startup (expensive — compiles Cranelift IR)
and shared across all WASM connectors. Each connector gets its own `Store`
with independent fuel counter and memory. When fuel runs out, execution traps.
When memory exceeds limit, allocation fails. No guest code can observe or
affect another guest's store.

**Capability enforcement at runtime:**

```rust
// crates/springtale-connector/src/native/capability.rs
// (WASM connectors get this automatically via host_api gating)

pub fn check_capability(
    manifest: &ConnectorManifest,
    action: &str,
    target: &CapabilityTarget,
) -> Result<(), CapabilityDenied> {
    // For every action, check that the manifest declares the required capability.
    // NetworkOutbound: exact host match, no wildcards
    // FilesystemRead/Write: path prefix match
    // ShellExec: requires signed CapabilityGrant token from user
    // KeychainRead: exact key name match
}
```

This check runs BEFORE every `execute()` call, for both native and WASM
connectors. The difference: WASM connectors are additionally memory-isolated.
Native connectors share the process but their capabilities are still enforced.

**External dependencies:** `wasmtime`, `wasmtime-wasi`, `serde`, `serde_json`,
`springtale-crypto` (signature verification), `springtale-store` (persistence),
`garde`, `tracing`, `thiserror`

**Security audit:**

| Concern | Control |
|---|---|
| WASM guest reads host memory | Impossible — Wasmtime linear memory is isolated. Guest cannot address host memory. |
| WASM guest exceeds CPU budget | Fuel metering traps execution when budget exhausted. |
| WASM guest exceeds memory | Memory limit enforced at page allocation. Trap on exceed. |
| Native connector bypasses capability check | `check_capability()` called in `execute()` dispatcher, not in connector code. Connector cannot skip it. |
| Manifest tampered after install | Signature verified at install. Stored manifest hash checked on every load. Mismatch → connector suspended. |
| TOCTOU on capability checks | Capability check and action dispatch are synchronous within the same `execute()` call. No window between check and use. |

**Privacy audit:**

| Concern | Control |
|---|---|
| Connector exfiltrates data to undeclared host | `NetworkOutbound` capability lists exact hosts. WASM `host_api` checks before any `connect()`. Native connector `reqwest` calls go through capability-checked wrapper. |
| Connector accesses another connector's data | Each connector has its own `PipelineContext`. No shared state between connectors. Registry queries filtered by connector name. |
| DataDisclosure not enforced | `DataDisclosure` in manifest is a transparency requirement — user sees what data the connector collects at install time. Enforcement is via capability grants (connector can only access what it declared). |

---

### 6.5 `springtale-scheduler`

**Purpose:** Cron execution, filesystem event watching, job queue (via springtale-store), retry
with backoff. Feeds events into `springtale-core`'s router.

**Key modules:**

| Module | Responsibility |
|---|---|
| `cron::executor` | Parse cron expressions, fire triggers via tokio timer |
| `watcher::fs_watcher` | `notify`-based filesystem watcher, debounced events |
| `queue::producer` | Enqueue jobs via `StorageBackend::enqueue_job()`, serialized as JSON |
| `queue::consumer` | Poll `StorageBackend::dequeue_job()` with concurrency limit, ack/nack pattern |
| `retry::backoff` | Exponential backoff with ±10% jitter, max attempts config |
| `heartbeat::monitor` | Phase 2: periodic wake cycle (configurable interval, default 30min) |
| `heartbeat::checklist` | Phase 2: evaluates rule conditions, decides whether to notify user |

**Heartbeat (Phase 2):** The heartbeat module replaces OpenClaw's HEARTBEAT.md
pattern with typed, sandboxed evaluation. Every interval (default 30 minutes),
the heartbeat executor runs a configured set of rules through `springtale-core`'s
pipeline engine. Each rule evaluates conditions (check email count, check CI
status, check calendar) and decides whether to fire a notification action.
Unlike OpenClaw's approach (raw AI prompt reading a Markdown checklist with
full machine access), Springtale's heartbeat runs each check through the
connector sandbox with capability enforcement. A heartbeat rule that checks
GitHub CI status can only access `api.github.com` — it cannot read your
filesystem or execute shell commands unless those capabilities are explicitly
declared and approved.

**External dependencies:** `tokio`, `springtale-store`, `notify`, `cron`, `chrono`,
`serde`, `serde_json`, `tracing`, `springtale-core` (trigger types)

---

### 6.6 `springtale-store`

**Purpose:** All persistence. **SQLite by default (local-first, zero external
dependencies).** PostgreSQL available as optional backend for multi-user/server
deployments. Storage backend selected by config — all queries go through the
`StorageBackend` trait. No raw SQL strings outside this crate.

```rust
// crates/springtale-store/src/backend/trait_.rs

#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    // Rules
    async fn insert_rule(&self, rule: &Rule) -> Result<RuleId>;
    async fn find_rules_by_trigger(&self, trigger: &TriggerType) -> Result<Vec<Rule>>;
    async fn toggle_rule(&self, id: RuleId, enabled: bool) -> Result<()>;

    // Connectors
    async fn register_connector(&self, manifest: &ConnectorManifest) -> Result<()>;
    async fn list_connectors(&self) -> Result<Vec<ConnectorEntry>>;

    // Events
    async fn log_event(&self, event: &EventEntry) -> Result<()>;
    async fn list_events(&self, filter: EventFilter) -> Result<Vec<EventEntry>>;
    async fn delete_events_before(&self, before: DateTime<Utc>) -> Result<u64>;

    // Jobs (replaces Redis queue)
    async fn enqueue_job(&self, job: &Job) -> Result<JobId>;
    async fn dequeue_job(&self) -> Result<Option<Job>>;
    async fn complete_job(&self, id: JobId) -> Result<()>;
    async fn fail_job(&self, id: JobId, error: &str) -> Result<()>;
}
```

**Key modules:**

| Module | Responsibility |
|---|---|
| `backend::trait_` | `StorageBackend` trait — all persistence goes through this |
| `backend::sqlite` | **Default.** Single-file SQLite via `rusqlite`. Zero external deps. WAL mode. |
| `backend::postgres` | **Optional** (`--features postgres`). `sqlx` PgPool for server deployments. |
| `schema::connectors` | Row type for `connectors` table |
| `schema::events` | Row type for `events` table |
| `schema::rules` | Row type for `rules` table |
| `schema::jobs` | Row type for `jobs` table (replaces Redis queue) |
| `schema::apply` | Declarative schema apply — `PRAGMA user_version`–fingerprinted, transactional, one `.sql` file per domain under `schema/sql/` |

**Why SQLite, not PostgreSQL, for Phase 1:**
The research says "no external queue dependency." Redis and PostgreSQL are external
services. A local-first automation tool should work as a single binary with a single
file. SQLite gives us ACID transactions, FTS5 search, WAL-mode concurrency, and
zero deployment burden. PostgreSQL is available for Phase 2 server deployments
where multiple users share one `springtaled` instance.

**Why no Redis:**
The job queue is implemented as a SQLite table with `dequeue_job()` using
`UPDATE ... RETURNING` with row locking. tokio tasks poll the queue. For
single-user local deployments this is sufficient. For multi-worker server
deployments, PostgreSQL's `SKIP LOCKED` provides the same pattern without Redis.

**External dependencies (default):** `rusqlite`, `serde`, `serde_json`, `uuid`, `chrono`, `tracing`
**External dependencies (postgres feature):** above + `sqlx`

**Security audit:**

| Concern | Control |
|---|---|
| Row-level isolation | Connector A cannot query connector B's events. All queries filter by `connector_id`. |
| SQLite file permissions | Created with `0o600`. WAL and SHM files inherit same permissions. |
| Query logging | TRACE level only. No PII at default log level. |
| Backup | SQLite file is a single file — standard backup. Sensitive payloads encrypted at app layer via `Secret<T>`. |

**Privacy audit:**

| Concern | Control |
|---|---|
| Event log content | Stores trigger type, connector name, timestamp, action taken. NOT trigger payload content. |
| Data retention | `delete_events_before(timestamp)` for retention enforcement. No automatic purge — user controls their data. |
| Database location | `~/.local/share/springtale/springtale.db` (XDG standard). |

---

### 6.7 `springtale-ai`

**Purpose:** The socket where the user plugs in their own AI. Springtale does not
provide AI. Springtale provides a bot that can optionally consume whatever AI the
user is already running — Ollama on their machine, a llama.cpp server, an OpenAI
API key they're already paying for, a Claude API key. The `AiAdapter` trait is
the interface. `NoopAdapter` is the default. The bot works without any AI plugged in.

This is the malwarevangelist-bot pattern: the LLM/agent is detachable. The bot
architecture stands on its own. AI enhances it when present, but the bot doesn't
depend on it. When the AI bubble pops, you unplug the adapter. Nothing breaks.

**AI has exactly two roles when plugged in:**

1. **NL→Rule parser (authoring convenience).** User says "notify me on Discord
   when my Kick stream goes live." The user's AI parses that into a structured
   `Rule` (TOML). The rule is deterministic from that point forward — AI is
   never called again during evaluation.

2. **Pipeline action (user's choice).** Users who want AI in their workflows
   can add `AiComplete` as a pipeline stage — "summarize this RSS feed" or
   "analyze this PR diff." This calls the user's own AI. If no AI is plugged in,
   the stage passes through `prev.output` unchanged. The workflow completes.

```rust
// crates/springtale-ai/src/adapter/trait_.rs

#[async_trait]
pub trait AiAdapter: Send + Sync + 'static {
    /// Generic text completion — calls the user's plugged-in AI.
    async fn complete(&self, request: AiRequest, options: AiOptions) -> Result<AiResponse>;

    /// Streaming variant.
    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream>;

    /// NL→Rule parser — uses the user's AI to parse intent into structured Rule.
    async fn parse_rule(&self, intent: &str, available_connectors: &[ConnectorInfo]) -> Result<Rule>;

    /// Check if the user has plugged in an AI and it's reachable.
    async fn is_available(&self) -> bool;
}
```

**Adapter implementations (thin clients to user-provided AI):**

| Module | What the user brings | What we provide |
|---|---|---|
| `noop::adapter` | Nothing — no AI | `NoopAdapter`: returns `Err(AiDisabled)`. Default. Zero deps. |
| `ollama::adapter` | Ollama running locally | Thin HTTP client to `localhost:11434` |
| `openai::adapter` | Any OpenAI-compatible API key (GPT, Gemini, Kimi, DeepSeek, OpenRouter) | Thin client to `/v1/chat/completions` |
| `anthropic::adapter` | Anthropic API key | Thin client to `/v1/messages` with tool_use |
| `voice::stt` | Whisper-compatible endpoint (local or remote) | Thin bridge: audio → text |
| `voice::tts` | ElevenLabs key or local Piper instance | Thin bridge: text → audio |

These are not "our AI." These are thin typed wrappers. The user configures
their endpoint in `springtale.toml` or via `springtale-cli ai configure`.
We never auto-discover AI endpoints. The user explicitly chooses.

**NL→Rule flow (when AI is plugged in):**

```
"Post to Discord when my Kick stream goes live in the gaming category"
        ↓
    parse_rule(intent, [connector-kick, connector-discord, ...])
        ↓   (calls the user's AI — whatever they plugged in)
    Rule {
        trigger: ConnectorEvent { connector: "kick", event: "stream_live" },
        conditions: [FieldEquals { field: "category", value: "gaming" }],
        actions: [RunConnector { connector: "discord", action: "send_message", ... }],
    }
        ↓
    User reviews → saves as rules/kick-to-discord.toml
        ↓
    RuleEngine runs it deterministically. No AI. No network. Forever.
```

**Key modules:**

| Module | Responsibility |
|---|---|
| `adapter::trait_` | `AiAdapter` trait — the plugin socket |
| `parser::rule_gen` | NL→Rule generation — prompt templates, connector context, Rule validation |
| `parser::prompt` | Prompt templates for structured output from the user's AI |
| `noop::adapter` | `NoopAdapter` — default, zero deps, zero network |
| `ollama::adapter` | Thin client to user's local Ollama |
| `openai::adapter` | Thin client to user's OpenAI-compatible endpoint |
| `anthropic::adapter` | Thin client to user's Anthropic endpoint |
| `voice::stt` | Bridge to user's STT endpoint |
| `voice::tts` | Bridge to user's TTS endpoint |

**External dependencies (noop feature):** `thiserror`, `async-trait`
**External dependencies (any adapter feature):** above + `reqwest`, `serde`, `tokio`, `tracing`

**Mobile — bring your own AI on phones and tablets (Phase 2b):**

The `AiAdapter` trait is the same on mobile. What changes is where the user's AI
lives. Four options, configured in the mobile app:

1. **Connect to home server (recommended).** Phone pairs with user's `springtaled`
   via QR code or mDNS. All AI calls route to the server's Ollama over the local
   network or VPN/Tailscale. Maximum privacy — data never leaves user's infra.
   The phone is a thin client to the same bot that runs on their desktop/server.

2. **On-device model.** llama.cpp via Tauri native plugin (Swift on iOS, Kotlin on
   Android). Small models (3-7B) run directly on the phone. Slower but fully
   offline. Good for users who want zero network dependency.

3. **Remote API key.** Same as desktop — user configures their OpenAI/Anthropic/etc
   endpoint. Data goes to their chosen provider.

4. **No AI.** `NoopAdapter`. Commands, rules, automations all work. Just no
   freeform conversation or NL→Rule. This is the default.

**Security audit:**
- Adapter only connects to user-configured endpoints. No default public endpoints. HTTPS validated (HTTP rejected unless `--allow-insecure-ai-endpoint`).
- `AiOptions { max_tokens: u32, timeout: Duration }` — user controls cost. Default: 4096 tokens, 30s timeout.
- Response body read with 10 MiB limit. `NoopAdapter` is default: no AI calls unless user explicitly configures.

**Privacy audit — the critical boundary. The user's data goes to the user's AI:**
- `AiRequest` is a closed enum. `Secret<T>` values cannot serialize into it (type system enforced). Connector output sanitized before reaching the adapter.
- The user explicitly configures where their data goes. No auto-discovery. No telemetry. No feedback loops. Their relationship with their AI provider is their own.
- Local-first: Ollama connects to `localhost:11434`. Data never leaves the machine unless the user configures a remote endpoint.
- Voice data processed and immediately discarded. Not persisted anywhere.

---

### 6.8 `springtale-mcp`

**Purpose:** Adapt the connector framework to the MCP protocol. Any `Connector`
becomes an MCP server. Replaces NosytLabs' hand-written per-connector MCP servers.

The MCP security model follows the spec's own guidelines (MCP Security Best
Practices) and CoSAI's MCP threat categories (published January 2026) rather
than inventing a parallel framework. Our connectors already run in a WASM
sandbox with capability enforcement — MCP inherits that same boundary.

**Key modules:**

| Module | Responsibility |
|---|---|
| `server::builder` | Build `rmcp::McpServer` from any `Connector` impl |
| `adapter::connector` | Translate connector actions → MCP tool definitions |
| `transport::stdio` | `StdioServerTransport` wrapper for CLI MCP use |
| `transport::sse` | SSE transport for remote/browser MCP consumption |

Schema generation uses `schemars` — `#[derive(JsonSchema)]` on action input
types generates JSON Schema for MCP tool definitions without duplication.

**MCP security — inherited from the connector sandbox, not a separate layer:**

The core insight: our connectors are already sandboxed (Wasmtime WASM, capability
grants, manifest signing). MCP tool calls route through the same `springtale-connector`
sandbox as direct connector calls. The MCP layer adds protocol-level controls
defined by the spec:

| MCP Threat (CoSAI/OWASP) | How our existing architecture handles it |
|---|---|
| **Tool poisoning** (malicious descriptions) | Tool descriptions generated from connector manifest `ActionDecl` — not user-editable at runtime. Schema hash recorded at install time in signed manifest. |
| **Rug pull** (schema changes post-install) | Connector manifest is Ed25519 signed. Schema hash is part of the signature. Runtime schema changes invalidate the signature → connector suspended. |
| **Cross-server shadowing** | Each MCP server wraps one connector. Connectors run in isolated WASM sandboxes with separate `PipelineContext`. No shared state between connectors. |
| **Sampling abuse** (server requests LLM completions) | MCP sampling disabled by default. Opt-in per connector in manifest. When enabled, sampling requests route through the user's AI adapter with same `AiRequest` boundary. |
| **Token passthrough** | No token passthrough. Each connector authenticates to its own API with its own credentials from the vault. MCP server never receives or forwards user tokens. |
| **Localhost trust assumption** | No implicit localhost trust. All MCP connections authenticated. (CVE-2026-25253 exploited this assumption in OpenClaw.) |
| **Input validation** | `garde` validation on all tool inputs. `schemars`-generated JSON Schema enforced before tool execution. |

This means we don't need custom `tool_guard`, `schema_lock`, `isolation`, or
`confused_deputy` modules. The connector sandbox IS the security layer. MCP
is a thin protocol bridge on top of it.

**External dependencies:** `rmcp`, `schemars`, `serde_json`, `tokio`, `tracing`,
`springtale-core`, `springtale-connector`

---

### 6.9 `springtale-bot`

**Purpose:** Classical bot runtime. This is the product users interact with.
Event loop receives from all connected platforms, command router matches
deterministically, handlers dispatch to connectors, state tracks conversations.
The rule engine from `springtale-core` runs inside the bot's event loop —
cron rules, webhook rules, filesystem watch rules all fire through the same
dispatch path.

**Key modules:**

| Module | Phase | Responsibility |
|---|---|---|
| `runtime::event_loop` | 1b | Main loop — receives events from all connectors, dispatches to router |
| `runtime::headless` | 1b | No Tauri, no GUI. Runs as daemon or in Docker. |
| `runtime::lifecycle` | 1b | Start, health check, graceful shutdown, signal handling |
| `router::prefix` | 1b | Prefix command match: `/search`, `/help`, `/remind`, `/status` |
| `router::pattern` | 1b | Regex and keyword pattern matching |
| `router::alias` | 1b | User-defined command aliases (persisted in SQLite) |
| `router::fallback` | 1b/2a | No match → "unknown command" (1b) or route to user's AI (2a) |
| `handler::registry` | 1b | Register handlers by command name, dispatch on match |
| `handler::builtin` | 1b | Built-in handlers: `/help`, `/status`, `/rules`, `/connectors` |
| `handler::connector` | 1b | Generic: route command to named connector action by convention |
| `state::session` | 1b | Per-user, per-channel conversation state (SQLite-backed) |
| `state::prefs` | 1b | User preferences: timezone, language, notification settings |
| `state::persona` | 1b | Bot persona config: name, response tone, template library |
| `memory::persistent` | 1b | SQLite-backed persistent memory (not Markdown files) |
| `memory::context` | 1b | Conversation context window management, sliding window |
| `memory::compaction` | 1b/2a | Truncate to N entries (1b) or AI summarize (2a when AI plugged in) |
| `identity::bot_id` | 1b | BotId: Ed25519 keypair + HKDF pseudonym per community |
| `orchestrator::recursive` | 2a | Recursive pipeline spawning (Clicky pattern, fuel budgets) |
| `orchestrator::subagent` | 2a | Spawn child agent with scoped capabilities |
| `bridge::atproto` | 2a | ATProto bot bridge (malwarevangelist-derived) |
| `bridge::rekindle` | 3 | Rekindle community member bridge (§31 APAS) |

**How the event loop works (Phase 1b):**

```rust
// crates/springtale-bot/src/runtime/event_loop.rs (simplified)

pub async fn run(bot: &Bot) -> Result<()> {
    loop {
        tokio::select! {
            // Chat messages from connectors (Telegram in 1b, more in 2a)
            Some(msg) = bot.connector_events.recv() => {
                match bot.router.route(&msg) {
                    RouteResult::Command(cmd, args) => {
                        let handler = bot.handlers.get(&cmd)?;
                        let response = handler.handle(args, &bot.ctx).await?;
                        bot.send_response(msg.reply_to(), response).await?;
                    }
                    RouteResult::NoMatch => {
                        // Phase 1b: "Unknown command. Try /help"
                        // Phase 2a: route to user's AI adapter if plugged in
                        bot.fallback.handle(msg, &bot.ctx).await?;
                    }
                }
            }
            // Rule engine events (cron, webhook, filesystem)
            Some(trigger) = bot.rule_events.recv() => {
                bot.rule_engine.evaluate_and_dispatch(trigger, &bot.ctx).await?;
            }
            // Scheduler events (heartbeat, queued jobs)
            Some(job) = bot.job_events.recv() => {
                bot.scheduler.execute(job, &bot.ctx).await?;
            }
        }
    }
}
```

**Command routing example (no AI, no network, instant):**

```
User sends: /search tokyo weather

router::prefix matches "/search" → SearchHandler
SearchHandler:
  1. Parse args: query = "tokyo weather"
  2. Call connector-presearch.execute("search", { query: "tokyo weather" })
  3. Connector calls Presearch API, returns results
  4. Format results using response template
  5. Send formatted message back to user

Total: one connector call. No AI involved. Instant.
```

**Handler registration (extensible by connectors):**

When a connector is installed, the bot auto-registers handlers for its actions.
`connector-presearch` with action `search` becomes available as `/search`.
`connector-github` with action `create_issue` becomes `/github create_issue`.
Users can alias these: `/gh` → `/github`, `/s` → `/search`.

Built-in handlers (`/help`, `/status`, `/rules`, `/connectors`) are always
available and cannot be overridden by connectors.

**External dependencies:** `springtale-core` (pipeline, rules), `springtale-crypto` (identity),
`springtale-connector` (dispatch), `springtale-store` (persistence), `springtale-transport`
(communication), `tokio`, `tracing`, `thiserror`

**Security audit:**

| Concern | Control |
|---|---|
| Command injection via chat message | Router matches on exact prefix. Args are passed as typed strings to handlers, never evaluated or interpolated. No shell expansion. |
| Handler calls connector outside declared capability | Handler calls `connector.execute()` which goes through capability check. Same enforcement as rule-triggered actions. |
| State poisoning across users | Session state keyed by `(user_id, channel_id)`. No cross-user state access. SQLite row-level isolation. |
| Bot persona impersonation | Bot identity is Ed25519 keypair. Responses signed in Phase 3 (Veilid). In Phase 1b/2a, platform-level bot identity (Telegram bot token) provides authenticity. |

**Privacy audit:**

| Concern | Control |
|---|---|
| Chat messages stored in plaintext | Conversation context stored in SQLite with app-layer encryption via vault. Context window is sliding — old messages pruned. |
| Bot logs contain message content | Message content logged at TRACE level only (disabled in production). Event log stores metadata only. |
| User preferences contain PII | Preferences stored in SQLite, encrypted at rest. User can export/delete via `/prefs export` and `/prefs reset`. |

---

### 6.10 `springtale-sentinel`

**Purpose:** Runtime behavioral monitor. Prevents bots and connectors from
exceeding their expected behavior. Phase 2a scope.

**Design — start simple, add sophistication later:**

Sentinel ships with hard constraints only (Layer 1). This covers the most
critical protections with zero false positives and no learning period:

| Control | What it does | How it works |
|---|---|---|
| **Rate limiter** | Prevents action flooding | Configurable actions/minute per connector. Default: 60. Exceed → throttle. |
| **Toxic pair blocker** | Prevents dangerous capability combinations | `KeychainRead + NetworkOutbound(different host)` blocked at install time. Configurable policy. |
| **Circuit breaker** | Prevents cascade failures | 3 consecutive failures on a pipeline stage → stage disabled, user notified. Auto-reset after configurable cooldown. |
| **Dead-man switch** | Prevents runaway autonomous execution | If > N actions/minute without user interaction → all pipelines pause. User must acknowledge to resume. |
| **Destructive action gate** | Prevents accidental damage | Actions tagged `impact: destructive` always require user approval regardless of autonomy level. |
| **Audit trail** | Enables post-incident review | Every action, decision, and connector call logged to append-only SQLite table. Exportable via CLI. |

**What's NOT in Phase 2a (deferred to later):**
- Statistical baseline learning (Layer 2) — requires defining "normal" for each bot, which varies wildly by use case
- Trajectory analysis (Layer 3) — research-grade anomaly detection on action sequences
- These are valuable but complex. Layer 1 alone provides more protection than OpenClaw's zero monitoring.

```rust
// crates/springtale-sentinel/src/lib.rs

pub struct Sentinel {
    rate_limiters: DashMap<ConnectorName, RateLimiter>,
    circuit_breakers: DashMap<PipelineStageId, CircuitBreaker>,
    dead_man: DeadManSwitch,
    policy: ToxicPairPolicy,
    audit: AuditTrail,
}

impl Sentinel {
    /// Called before every pipeline action. Returns verdict.
    pub async fn evaluate(&self, action: &Action, connector: &str) -> Verdict {
        // 1. Rate limit check
        // 2. Circuit breaker check
        // 3. Dead-man switch check
        // 4. Destructive action gate
        // → Go | Throttle | Pause | Quarantine
    }

    /// Called at connector install time.
    pub fn check_toxic_pairs(&self, manifest: &ConnectorManifest) -> Result<(), ToxicPairViolation> { }

    /// Export audit trail.
    pub async fn export_audit(&self, range: TimeRange) -> Vec<AuditEntry> { }
}

pub enum Verdict {
    Go,
    Throttle(Duration),
    Pause(String),           // user notification required to resume
    Quarantine(String),      // connector isolated — admin review needed
}
```

**External dependencies:** `springtale-core` (action types), `springtale-store` (audit persistence),
`dashmap`, `tokio`, `tracing`, `thiserror`

---

## 7. Connector Crates

All first-party connectors follow this internal structure:

```
connector-{name}/src/
  lib.rs              # pub use, connector struct registration
  config.rs           # Config struct, all secrets as Secret<String>
  auth/               # Auth flows (OAuth2, API key, bearer token)
  client/             # Typed API client (no raw reqwest in other modules)
  triggers/           # One module per trigger type
  actions/            # One module per action type
```

**Shared rules across all connectors:**
- Config structs derive `serde::Deserialize` only — never `Serialize` (prevents secrets in logs)
- `Secret<String>` for all credentials, API keys, tokens
- All network calls use `reqwest` with `rustls-tls` — no OpenSSL paths
- HMAC/signature verification on all incoming webhooks (where platform supports it)
- Typed error enums via `thiserror` — no `anyhow` in connector library code
- Every action has a corresponding `#[cfg(test)]` module with mock client tests

| Connector | Replaces | Key Capabilities |
|---|---|---|
| `connector-kick` | NosytLabs KickMCP | Chat, stream events, channel info, OAuth2 PKCE |
| `connector-presearch` | NosytLabs presearch MCP | Privacy search, scraping, result cache, multi-language |
| `connector-bluesky` | NosytLabs (none) | ATProto sessions, Jetstream firehose, post/reply |
| `connector-github` | NosytLabs (none) | REST + GraphQL, webhook HMAC verify, issues, PRs |
| `connector-filesystem` | NosytLabs (none) | notify watcher, sandboxed read/write, path allow-list |
| `connector-shell` | NosytLabs (none) | Command allow-list, timeout, ShellExec capability gate |
| `connector-http` | NosytLabs (none) | Generic HTTP client, webhook ingest, rustls-tls |
| `connector-telegram` | — (Phase 1b) | Bot API typed client, inline keyboards, file upload, webhook mode. First chat connector — enables bot use via chat. |

### Phase 2a Chat Connectors

These replace OpenClaw's remaining channel adapters (Baileys, discord.js, etc.)
with sandboxed Rust implementations. Each follows the same `connector-{name}/src/`
structure as Phase 1 connectors.

| Connector | OpenClaw Equiv. | Key Capabilities |
|---|---|---|
| `connector-discord` | discord.js (npm) | Gateway WebSocket, slash commands, voice channel join, embed builder |
| `connector-signal` | signal-cli (Java) | signal-cli bridge, E2E encrypted messages, disappearing messages |
| `connector-whatsapp` | Baileys (npm) | Baileys-equivalent protocol, sandboxed, QR code pairing |
| `connector-matrix` | — | Matrix SDK, federated rooms, E2E encryption, room state |
| `connector-irc` | — | Raw TCP + rustls-tls, lightweight, channel join/part/msg |
| `connector-slack` | Bolt (npm) | Socket mode, slash commands, blocks, thread replies |
| `connector-browser` | Built-in headless Chrome | Chromium via `headless_chrome` crate, form fill, scrape, navigate, sandboxed |
| `connector-nostr` | — | NIP-01 relay client, event signing, DM encryption (NIP-04/NIP-44) |
| `connector-rekindle` | — (Phase 3) | **P2P AI chat interface.** DM mode: E2E encrypted 1:1 with user (Chiralagram §27). Community mode: bot member in channels (§31). QR code pairing. Zero metadata leakage. No phone number. No server. |

**`connector-browser` (Phase 2, high priority):** Replaces OpenClaw's built-in
browser automation. Uses the `headless_chrome` Rust crate (Chrome DevTools
Protocol) instead of Playwright (npm). Runs inside the Wasmtime sandbox with
declared capabilities:
- `Capability::NetworkOutbound { host: "<user-approved-domain>" }` per domain
- `Capability::BrowserNavigate` — navigate to approved domains only
- `Capability::BrowserFormFill` — fill form fields (no credential autofill)
- `Capability::BrowserScreenshot` — capture page screenshots

Unlike OpenClaw where the AI has unrestricted browser access, Springtale's
browser connector declares every domain it contacts. The user approves the
domain list at install time. The connector cannot navigate to unapproved sites.

**Critical difference from OpenClaw:** All Phase 2 connectors run inside the
same Wasmtime sandbox as community connectors (when authored in TypeScript)
or as `NativeConnector` with declared capabilities (when first-party Rust).
OpenClaw's Baileys implementation, for example, runs with full filesystem
and network access. Springtale's `connector-whatsapp` declares exactly which
hosts it contacts (`Capability::NetworkOutbound { host: "web.whatsapp.com" }`)
and cannot access any other network endpoint.

### Phase 3 Distributed Connector Registry

When VeilidTransport is active, the connector registry migrates from
PostgreSQL to Veilid DHT records. This eliminates the central database
dependency.

### Phase 3 `connector-rekindle` — P2P AI Chat Interface

**This is the primary Phase 3 use case.** Instead of chatting with your
agent through Telegram (like OpenClaw), you open an E2E encrypted DM
with your Springtale bot inside Rekindle. No central server. No metadata
leakage. No phone number required.

**How it works (Rekindle §27 Chiralgrams + §31 Bot API):**

```
USER (Rekindle client)              SPRINGTALE BOT (headless Veilid node)
│                                   │
│ 1. Open DM with bot               │
│    Create 2-party SMPL record     │
│    ECDH(user_pseudo, bot_pseudo)  │
│    → DM key (X25519)              │
│                                   │
│ 2. Write message to subkey 0      │
│    "Schedule my meeting tomorrow" │
│    XChaCha20-Poly1305 encrypted          │
│                                   │
│    ────── gossip + SMPL write ──► │
│                                   │
│                                   │ 3. connector-rekindle receives
│                                   │    via watch on DM record
│                                   │    Decrypt with DM key
│                                   │    Route through pipeline:
│                                   │    Trigger → Condition → AI Adapter
│                                   │    → Action (connector-google-cal)
│                                   │
│                                   │ 4. Write response to subkey 1
│                                   │    "Done. Meeting scheduled for
│                                   │     tomorrow 2pm with Bob."
│                                   │    XChaCha20-Poly1305 encrypted
│                                   │
│ ◄── gossip + watch callback ──── │
│                                   │
│ 5. User sees response in          │
│    Rekindle DM window             │
```

**Why this is better than Telegram/WhatsApp/Discord for AI chat:**

| Property | OpenClaw via Telegram | Springtale via Rekindle |
|---|---|---|
| Encryption | Telegram server sees plaintext | E2E encrypted (DM key via ECDH). Nobody can read except user + bot. |
| Metadata | Telegram knows who, when, how often | Veilid private routes. No IP correlation. No phone number. |
| Server dependency | Telegram's servers must be up | P2P. Works if user + bot are on same LAN, same Veilid network, or anywhere. |
| Account requirement | Phone number required | Pseudonym only. Derived from master key. Unlinkable. |
| Message persistence | On Telegram's servers, subject to law enforcement requests | On user's local SQLite + bot's local SQLite. Nowhere else. |
| Data sovereignty | Telegram ToS governs your data | User owns everything. `springtale-cli data purge` deletes it all. |
| Conversation security | OpenClaw stores chat history in plaintext Markdown | Encrypted at rest (vault + SQLite encryption). |
| Bot identity | Username on Telegram | Ed25519 keypair with HKDF pseudonym. Cryptographically verified. |

**`connector-rekindle` key modules:**

| Module | Responsibility |
|---|---|
| `dm.rs` | Create/accept DM SMPL records with user. ECDH key derivation. Message encrypt/decrypt. |
| `channel.rs` | Join community channels as bot member. Watch subkeys for mentions/commands. |
| `presence.rs` | Write `MemberPresence` to registry (online/offline, current task). |
| `triggers.rs` | `DmReceived`, `ChannelMention`, `GovernanceChange` trigger types. |
| `actions.rs` | `SendDm`, `SendChannelMessage`, `UpdatePresence` action types. |
| `pairing.rs` | QR code / deep link pairing: user's Rekindle client discovers bot's route_blob → initiates DM. |

**Pairing flow (first time):**
1. User runs `springtale-cli bot pair --via rekindle` → displays QR code containing
   bot's public key + route_blob.
2. User scans QR in Rekindle client → initiates DM invite (§27 DMInvite).
3. Bot accepts → 2-party SMPL record created. DM key derived via ECDH.
4. User types first message → connector-rekindle routes through pipeline.
5. Future sessions: DM record key is stored in both SQLite databases. No re-pairing needed.

**Community bot mode (public channels):**
Same bot can also operate in community channels (§31 pattern). It joins via
`InviteSecrets`, gets a member slot, watches channels for mentions or commands.
Responses written to its subkey in the channel record. Community members see
bot responses in the normal chat flow.

**Both modes simultaneously:** A single `springtaled` instance can run
`connector-rekindle` in DM mode (private AI chat) AND community mode
(public bot in channels). Same pipeline, same AI adapter, same connector
sandbox. The bot just has multiple SMPL records it watches.

**Registry DHT Record:** A single SMPL record (same universal schema as
Rekindle: `o_cnt: 0`, 255 member subkeys) stores connector manifests.
Each registered connector author writes their signed manifest to their
own subkey. Readers verify Ed25519 signatures before loading.

**Migration path:**
1. `springtale-store` registry queries gain a `RegistryBackend` trait abstraction.
2. `PgRegistryBackend` wraps existing sqlx queries (Phases 1-2).
3. `DhtRegistryBackend` wraps Veilid DHT read/write (Phase 3).
4. `runtime::boot` selects backend based on configured transport.
5. One-time migration: `springtale-cli registry migrate --to-dht` exports
   PostgreSQL registry contents to DHT records.

**Connector discovery:** Agents discover connectors by reading the registry
DHT record and verifying manifests. The Veilid DHT's natural replication
ensures availability without a central server. Connector updates propagate
via DHT `watch_dht_values()` — the same mechanism Rekindle uses for
governance change notification.

---

## 8. Applications

### 8.1 `springtaled` — Headless Daemon

Self-hostable server process. Docker-first deployment. Manages connector
lifecycle, rule evaluation, scheduler, and the management HTTP API.

**Startup order** (enforced in `runtime::boot`):
1. Load config from `springtale.toml` (TOML, `Secret<T>` fields)
2. Initialize `springtale-store` (SQLite default, PostgreSQL if configured)
3. Initialize `springtale-crypto` vault — prompt for unlock passphrase if needed
4. Initialize `springtale-transport` with configured transport impl
5. Initialize `springtale-scheduler` (uses springtale-store for job queue)
6. Initialize `springtale-sentinel` behavioral monitor — begins baseline collection
7. Load and verify all enabled connectors from registry
8. Start `springtale-scheduler` cron + watcher + heartbeat tasks
9. Start axum management API
10. Signal readiness (stdout `READY\n` for process supervisors)

**Management API routes (axum):**

| Method | Path | Description |
|---|---|---|
| GET | `/health` | Liveness probe |
| GET | `/ready` | Readiness probe |
| GET | `/connectors` | List all connectors |
| POST | `/connectors/install` | Install connector from manifest |
| DELETE | `/connectors/{name}` | Remove connector |
| POST | `/connectors/{name}/enable` | Enable connector |
| POST | `/connectors/{name}/disable` | Disable connector |
| GET | `/rules` | List all rules |
| POST | `/rules` | Create rule |
| PUT | `/rules/{id}` | Update rule |
| DELETE | `/rules/{id}` | Delete rule |
| POST | `/rules/{id}/run` | Manual rule trigger |
| GET | `/events` | Event log (paginated) |
| POST | `/webhook/{connector}/{trigger}` | Inbound webhook endpoint |

**Security audit for `springtaled`:**

| Concern | Control |
|---|---|
| API authentication bypass | All routes except `/health` and `/ready` require HMAC bearer token. Token derived from vault passphrase hash — no separate API key to manage. |
| Webhook injection (unauthenticated) | `/webhook/{connector}/{trigger}` verifies HMAC-SHA256 signature from header before processing. Rejects if connector doesn't declare webhook support in manifest. |
| Startup race conditions | Strict ordered boot (1-10 above). Each step must succeed before the next starts. API is the LAST thing to start — no requests accepted during boot. |
| Management API DoS | `tower-http::limit` rate limiting: 100 req/s default. Request body size limit: 1 MiB. Timeout: 30s per request. |
| Privilege escalation via API | `POST /connectors/install` requires manifest signature verification. `ShellExec` capability triggers deferred approval — API returns 202 Accepted, not 200. Actual load waits for user approval via Tauri modal or CLI confirm. |
| Config file injection | `springtale.toml` parsed by `figment` with strict schema validation (`garde`). Unknown fields rejected. `Secret<T>` values read from env vars or vault, never from TOML file directly. |
| Process signal handling | SIGTERM triggers graceful shutdown: stop accepting requests, drain active pipelines (30s timeout), close DB connections, close vault. SIGKILL is unrecoverable — vault is already encrypted at rest. |

**Privacy audit for `springtaled`:**

| Concern | Control |
|---|---|
| API responses leak internal state | Error responses return opaque error codes, not stack traces. No internal types serialized to API consumers. |
| Event log contains PII | Event log stores metadata (trigger type, connector, timestamp, action taken). Trigger payload content NOT stored in event log — ephemeral in `PipelineContext`. |
| Bind address default | `127.0.0.1:8080` (management API) and `127.0.0.1:8081` (dashboard). Config validation emits WARNING if changed to `0.0.0.0`. |
| Docker environment | Non-root user (`USER 1000:1000`). Read-only root filesystem (`--read-only`). Secrets via Docker secrets or mounted vault file, never env vars. Healthcheck via `/health`. No capabilities (`--cap-drop=ALL`). |

### 8.2 `springtale-cli`

Local CLI runner. Uses `clap` derive with subcommands. Output in table
format (default) or JSON (`--json` flag) via `tabled` + `serde_json`.

```
springtale connector install <path-to-manifest>
springtale connector list
springtale connector remove <name>
springtale connector enable <name>
springtale connector disable <name>
springtale rule add --trigger cron --schedule "0 9 * * *" --action notify --title "Morning"
springtale rule list
springtale rule toggle <id>
springtale rule run <id>
springtale events --limit 50 --connector kick
springtale server start          # starts springtaled inline (dev mode)
springtale memory audit           # inspect + purge memory entries
springtale memory compact         # force context compaction
springtale registry migrate --to-dht  # Phase 3: PostgreSQL → Veilid DHT
```

**Security audit for `springtale-cli`:**

| Concern | Control |
|---|---|
| Passphrase in terminal history | Vault passphrase read via `rpassword` crate (no echo, not stored in shell history). Never passed as CLI argument. |
| `agent set-autonomy` privilege escalation | Autonomy elevation requires vault unlock (passphrase). Cannot elevate above connector's `max_autonomy` from manifest. L3/L4 require explicit `--confirm-autonomous` flag. |
| Config file injection via `--config` | Custom config paths validated: must be absolute path, must exist, must be owned by current user. Symlinks rejected. |
| `data export` output in plaintext | `data export` writes to stdout or `--output` file. File created with `0o600` permissions. WARNING emitted if output path is in a shared directory. `--encrypt` flag available to wrap output in vault-encrypted blob. |
| `rule add` from untrusted TOML | TOML parsed with strict schema validation. Template variables sanitized (no nested `${}`). `ShellExec` actions in imported rules trigger confirmation prompt. |

**Privacy audit for `springtale-cli`:**

| Concern | Control |
|---|---|
| Command history | Sensitive subcommands (`crypto`, `data purge`, `agent set-autonomy`) are NOT added to shell history if `HISTCONTROL=ignorespace` is set (commands prefixed with space). Documentation recommends this setup. |
| `data export` may contain PII | Output contains user's own data. WARNING header included in export: "This file may contain personal information." |
| `memory audit` displays conversation content | Conversation content shown in terminal. No automatic logging. Requires vault unlock to access. |

---

## 9. Tauri Desktop & Mobile Shell

**Phase 2 scope.** Mirrors Rekindle's four-layer stack exactly.
Targets: macOS, Windows, Linux, iOS, Android — one codebase, five platforms.

```
SolidJS + TypeScript Frontend
  Rendering, state display, user input
  No business logic. No secrets. No crypto.
  ipc/ module: typed invoke() wrappers only — no raw tauri.invoke strings
──────────────────────────────────────────
Tauri 2 IPC Bridge
  Commands: Frontend → Rust (user actions)
  Events:   Rust → Frontend (state updates)
  Window management, system tray, plugins
  Mobile: Swift (iOS) + Kotlin (Android) plugin bindings
──────────────────────────────────────────
Tauri Commands (src-tauri/commands/)
  Thin layer. Validates inputs. Delegates to crates.
  Never contains business logic directly.
──────────────────────────────────────────
Core Crates (same as springtaled)
  springtale-core, springtale-crypto, springtale-connector
  springtale-scheduler, springtale-store, springtale-ai
  All pure Rust. Zero Tauri dependency.
  Independently testable outside desktop context.
```

**Mobile-specific features (Tauri 2 plugins):**
- Camera access for QR code scanning (device pairing, connector install)
- Biometric authentication (Touch ID / Face ID / Android fingerprint)
- Push notifications via platform-native APIs
- Voice input via device microphone → `springtale-ai` STT module (user's STT endpoint)
- Geolocation for location-aware automation rules
- NFC tag reading for physical-world triggers
- Device pairing: QR code or Bonjour/mDNS for LAN discovery of `springtaled`
- On-device AI: llama.cpp via Tauri native plugin for offline inference (3-7B models)
- Home server AI: pair with `springtaled` instance → route AI calls to user's Ollama over LAN/VPN

**Canvas/A2UI (Phase 2):**
A live UI surface that the agent can programmatically push content to.
The agent writes structured data to a `Canvas` state object via IPC events;
the SolidJS frontend renders it reactively. Use cases: dashboards, data
tables, generated diagrams, interactive forms the agent builds on-the-fly.
Unlike OpenClaw's Canvas which renders in a browser tab, Springtale's Canvas
is a native Tauri window — same security boundary as the rest of the app.

**`springtale-dashboard` — Web Control UI (Phase 2):**
Lightweight SPA served by `springtaled` on the management API port.
Shares the SolidJS component library with the Tauri frontend but runs
standalone in any browser — for headless/remote server management.
- Connector status, rule management, event log viewer
- Heartbeat schedule configuration, session viewer
- No sensitive operations without HMAC bearer auth
- Replaces OpenClaw's Gateway Control UI on :18789
- Bind to `127.0.0.1` by default — never `0.0.0.0` (OpenClaw's default)

**SolidJS conventions (src/):**
- One component per file, named exports only
- State in SolidJS stores (`createStore`) — no prop drilling
- `ipc/` module exports typed async functions wrapping `invoke()`
- `styles/` contains global CSS only — no inline `style=` props
- Tailwind 4 utility classes in `class=` — no `@apply` except in `styles/`

**Security audit for Tauri shell:**

| Concern | Control |
|---|---|
| CSP policy | Strict Content-Security-Policy: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:;`. No external script loading. No eval. |
| IPC message validation | All Tauri commands in `src-tauri/commands/` validate inputs with `garde` before delegating to crates. No raw `serde_json::Value` accepted from frontend. |
| Deep link injection | Deep links (`springtale://`) parsed with strict schema validation. Only `connector-install` and `bot-pair` intents accepted. Malformed URLs rejected, not passed to handlers. |
| WebView isolation | Tauri 2 runs WebView in separate process. WebView cannot access Rust memory directly — only through IPC commands. No `window.__TAURI__` global in production build. |
| Canvas content injection | Canvas receives structured data (typed SolidJS stores), not raw HTML. No `innerHTML` or `dangerouslySetInnerHTML`. React-style XSS prevention via DOM API. |
| Approval modal spoofing | Capability approval and destructive action modals are native Tauri dialogs (`tauri::api::dialog`), not WebView DOM elements. Cannot be spoofed by frontend JavaScript. |
| Mobile: biometric bypass | Biometric auth failure falls back to vault passphrase, never to "no auth". Failed biometric attempts rate-limited by OS. |

**Privacy audit for Tauri shell:**

| Concern | Control |
|---|---|
| Clipboard access | No automatic clipboard access. Clipboard read/write only via explicit user action (paste, copy button). No background clipboard monitoring. |
| Screen recording | No screen capture capability in the app. Canvas is rendered content, not a screen share. |
| Local storage | No `localStorage` or `sessionStorage`. All state in SolidJS stores (memory only) or Rust-side vault/store. WebView storage cleared on app exit. |
| Telemetry | Zero telemetry. No analytics. No crash reporting to external services. Crash logs stored locally only. |
| Mobile: location/camera | Requested only when needed (QR scan, location rule). iOS/Android permission prompts shown by OS. Permissions revocable. Location data never persisted — used only for the triggering rule evaluation. |

**Security audit for `springtale-dashboard`:**

| Concern | Control |
|---|---|
| Session fixation | Stateless: HMAC bearer token per request. No server-side sessions. No cookies. Token in `Authorization` header only. |
| CSRF | No cookies = no CSRF risk. All mutation via POST/PUT/DELETE with bearer token. |
| XSS | SolidJS auto-escapes all rendered content. No `innerHTML`. CSP enforced by `springtaled` response headers. |
| Clickjacking | `X-Frame-Options: DENY` and `frame-ancestors 'none'` in CSP. |
| URL parameter PII | No PII in URL paths or query parameters. Rule IDs and connector names are non-sensitive identifiers. |
| WebSocket | No WebSocket. Dashboard polls API via standard HTTP. SSE for event log streaming (read-only, auth required). |

---

## 10. TypeScript Connector SDK

**Target audience:** Community connector authors who prefer TypeScript.
**Runtime:** Wasmtime WASM Component Model sandbox — never trusted host code.
**Toolchain:** `@bytecodealliance/jco` compiles TypeScript → `wasm32-wasi` component.

The SDK provides:
- `BaseConnector` abstract class (mirrors Rust `Connector` trait)
- `ConnectorManifest` schema (Zod v4, matches Rust manifest types)
- `Capability` enum (string-typed, mirrors Rust enum variants)
- Host API bindings (`host/api.ts`) — typed wrappers for WASI imports

Community connectors compile to `.wasm` components and publish a signed
`connector.toml` manifest. The host runtime verifies the signature and
loads the component into its sandbox.

**TypeScript standards (sdk only):**
- Node.js 24 LTS, ESM only, `"moduleResolution": "bundler"`
- TypeScript 5.7+, `strict: true`, `noUncheckedIndexedAccess: true`
- Zod v4 for runtime validation
- Biome 2 for linting + formatting (replaces ESLint + Prettier)
- Vitest 2 for testing (ESM-native)
- `pnpm` for package management

---

## 11. Transport Abstraction & Phase Plan

```rust
// crates/springtale-transport/src/transport/trait_.rs

use async_trait::async_trait;
use crate::identity::NodeId;

#[derive(Debug, Clone)]
pub struct Message {
    pub id:        uuid::Uuid,
    pub payload:   Vec<u8>,       // already encrypted by springtale-crypto at call site
}

#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a message to a peer node.
    async fn send(&self, to: &NodeId, msg: Message) -> anyhow::Result<()>;

    /// Receive the next inbound message. Blocks until available.
    async fn recv(&self) -> anyhow::Result<(NodeId, Message)>;

    /// Return this node's identity.
    async fn node_id(&self) -> NodeId;

    /// Human-readable transport name for logging.
    fn name(&self) -> &'static str;
}
```

| Phase | Transport | Status | Notes |
|---|---|---|---|
| 1 | `LocalTransport` | Active | tokio Unix sockets, same machine |
| 2 | `HttpTransport` | Planned | axum server + reqwest, mTLS, LAN/VPN |
| 3 | `VeilidTransport` | Stub → Phase 3 | Three-path delivery via `rekindle-protocol` (see §6.3) |

All crates accept `Arc<dyn Transport>`. The switch from Phase 1 to Phase 3
is: instantiate `VeilidTransport` instead of `LocalTransport` in `runtime::boot`.
Zero other changes.

**Phase 3 transport summary (detailed implementation in §6.3):**

`VeilidTransport` wraps Rekindle's three-path delivery model:
1. **SMPL Write** (durable) — message persists on DHT for offline catchup.
2. **Gossip** (fast) — `app_message` via private routes, sub-second to online peers.
3. **Watch + Inspect** (consistency) — `watch_dht_values` + periodic inspect catches gaps.

Any single path succeeding delivers the message. Receiver deduplicates by
`message_id`, sorts by Lamport timestamp, and returns via `Transport::recv()`.

Veilid's VICE layer handles NAT traversal transparently (direct → hole-punch
→ signal-reverse → relay). No IP addresses leak above the transport layer.
Privacy routes provide sender anonymity; receiver routes provide receiver
anonymity. Safety hops are configurable per message sensitivity.

**Phase 3 connector registry:** Migrates from PostgreSQL to Veilid DHT (see §7).
`RegistryBackend` trait abstracts the storage layer. `runtime::boot` selects
the backend based on the configured transport.

---

## 12. Dev Environment

Mirrors Rekindle's Nix flake setup via Konductor.

```nix
# flake.nix (abbreviated)
{
  inputs.konductor.url = "github:braincraftio/konductor";

  outputs = { konductor, ... }: {
    devShells.default = konductor.lib.mkShell {
      packages = [
        pkgs.rustup       # Rust stable via rustup
        pkgs.cargo-nextest  # faster test runner
        pkgs.cargo-watch  # cargo watch -x test
        pkgs.sqlx-cli     # sqlx migrate run
        pkgs.postgresql_16
        pkgs.redis
        pkgs.nodejs_24    # for connector-sdk-ts
        pkgs.pnpm
        pkgs.wabt         # WASM binary toolkit (wasm-validate)
        # ── Security tooling (see SECURITY.md) ──
        pkgs.cargo-deny   # license + advisory policy
        pkgs.cargo-audit  # RustSec advisory DB
        pkgs.cargo-geiger # unsafe code audit
        pkgs.cargo-vet    # supply chain audit trail
        pkgs.cargo-auditable # embed dep info in binaries
        pkgs.semgrep      # SAST pattern rules
        pkgs.gitleaks     # secrets detection in git
        pkgs.trufflehog   # verified secrets scanning
        pkgs.trivy        # container + binary CVE scanning
        pkgs.hadolint     # Dockerfile linting
        pkgs.cosign       # Sigstore artifact signing
        pkgs.syft         # SBOM generation for containers
      ];
    };
  };
}
```

**Quality gates (CI, GitHub Actions):**

```
# ── Code quality ───────────────────────────
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc
sqlx migrate run && cargo sqlx prepare --check
pnpm -C sdk/connector-sdk-ts run check  # tsc + biome
pnpm -C sdk/connector-sdk-ts run test

# ── Security (see SECURITY.md for full pipeline) ───
semgrep --config=p/owasp-top-ten --config=.semgrep/
cargo deny check                        # license + advisory policy
cargo audit                             # RustSec advisory DB
cargo vet                               # supply chain audit trail
cargo geiger --all-features             # unsafe audit
gitleaks detect --source=.              # secrets in repo
hadolint Dockerfile
trivy image springtale:latest --severity HIGH,CRITICAL
docker compose build --no-cache
```

No PR merges without all gates passing. The complete security pipeline
including DAST, fuzzing, SBOM generation, and artifact signing is
documented in SECURITY.md §9.1.

---

## 13. Ecosystem & Prior Art

Springtale does not exist in a vacuum. It draws from and contributes to a
constellation of projects under the `scopecreep-zip` org and allied repos.
Every design decision is informed by prior art that was built, tested, and
sometimes abandoned.

### 13.1 Rekindle (scopecreep-zip/rekindle)

Veilid-native decentralized gaming chat. P2P messaging is working. The
community mesh (governance CRDT, channel records, member registry) is in
active development per the v3.0 architecture document.

**What Springtale inherits:**
- `rekindle-protocol` crate — Phase 3 VeilidTransport wraps this directly.
- Four-layer Tauri stack (SolidJS → IPC Bridge → Commands → Pure Rust Crates).
  Springtale's desktop shell mirrors this architecture exactly (§9).
- Konductor-based Nix flake dev shell. Shared toolchain config.
- Ed25519 identity model. `springtale-crypto` reuses the same keypair generation
  and HKDF pseudonym derivation patterns.
- Signal Protocol crypto primitives for future E2E agent-to-agent messaging.

**What Springtale contributes back:**
- Connector framework becomes available to Rekindle bots (headless members
  can run connectors to bridge external services into communities).
- MCP adapter enables Rekindle bots to expose community data as MCP tools
  for AI coding assistants.

### 13.2 malwarevangelist-bot (radicalkjax/malwarevangelist-bot)

ATProto bot for Bluesky automation. The `feature/opencode-api` branch
integrates OpenCode API patterns for headless AI agent operation.

**What Springtale inherits:**
- ATProto session management patterns → `connector-bluesky` auth module.
- Jetstream WebSocket firehose consumer → proven in production, ported to
  typed Rust in `connector-bluesky/firehose/jetstream.rs`.
- Bot lifecycle management (startup, reconnection, graceful shutdown).
- OpenCode API integration patterns for headless agent orchestration.

### 13.3 Clicky (goldenapplestudios/Clicky)

Claude subagent pentesting workflow. Recursive bot framework using Skills
and scripts for multi-step automated workflows.

**What Springtale inherits:**
- Recursive pipeline pattern — Clicky's skill→script→subagent spawning
  maps directly to `springtale-core`'s `compose_pipeline()` with retry graph.
  Each pipeline stage can spawn sub-pipelines, creating recursive execution
  trees with fuel metering at every level.
- Skills-as-modules pattern — Clicky's skill files inform the connector
  manifest structure. Skills declare capabilities; the runtime enforces them.
- Subagent orchestration — multiple agent instances coordinating through
  shared state informs the `springtale-bot` crate's multi-bot architecture.

### 13.4 Konductor (braincraftio/konductor)

Nix flake framework for reproducible dev environments. Both Rekindle and
Springtale derive their `flake.nix` from Konductor's `mkShell`.

**Role in the ecosystem:**
- Single source of truth for Rust toolchain versions across all projects.
- Ensures `cargo-nextest`, `cargo-deny`, `sqlx-cli`, and `wabt` are
  identical between Rekindle and Springtale dev environments.
- Contributors to any project get the same shell: `direnv allow` and go.

### 13.5 Kalilix (usrbinkat)

Linux distribution and security tooling. Kat's repos include Konductor
and related infrastructure.

**What Springtale inherits:**
- Konductor Nix flake patterns (shared upstream).
- Container-first deployment patterns informing Docker Compose config.
- Security-conscious default configurations.

### 13.6 SpiritStream (scopecreep-zip/SpiritStream)

Guerrilla live streaming app. Tauri + React frontend with embedded server
sidecar. Demonstrates the Tauri desktop pattern in production.

**What Springtale inherits:**
- Tauri 2 build pipeline and desktop bundling patterns.
- Multi-platform CI (macOS, Windows, Linux) configuration.

---

## 14. Bot & Agent Framework

### 14.1 Design Philosophy

**A bot is an agent without a GUI.** The same `springtale-core` pipeline engine,
the same `springtale-connector` sandbox, the same `springtale-crypto` identity — just
without a Tauri window. This is the same philosophy as Rekindle's §31:
"A bot is a headless Veilid node with a community member slot."

No special infrastructure. No privileged nodes. Bots are agents. Agents are
peers. The framework makes no distinction at the protocol level.

### 14.2 `springtale-bot` Crate (Phase 1b)

```
crates/springtale-bot/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── runtime/
    │   ├── mod.rs
    │   ├── event_loop.rs      # Main bot event loop — receives from all connectors
    │   ├── headless.rs        # Headless runtime (no Tauri, no GUI)
    │   └── lifecycle.rs       # Start, health check, graceful shutdown
    ├── router/                # Phase 1b: classical command routing (no AI)
    │   ├── mod.rs
    │   ├── prefix.rs          # Prefix commands: /search, /help, /remind
    │   ├── pattern.rs         # Regex/keyword pattern matching
    │   ├── alias.rs           # User-defined command aliases
    │   └── fallback.rs        # No match → "unknown command" (Phase 1b) or AI (Phase 2a)
    ├── handler/               # Phase 1b: command handlers
    │   ├── mod.rs
    │   ├── registry.rs        # Handler registration + dispatch
    │   ├── builtin.rs         # Built-in handlers: /help, /status, /rules, /connectors
    │   └── connector.rs       # Generic handler: route command to named connector action
    ├── state/                 # Phase 1b: conversation state
    │   ├── mod.rs
    │   ├── session.rs         # Per-user, per-channel session tracking
    │   ├── prefs.rs           # User preferences (timezone, language, notification)
    │   └── persona.rs         # Bot persona config (name, tone, response templates)
    ├── memory/                # Phase 1b: persistent memory
    │   ├── mod.rs
    │   ├── persistent.rs      # SQLite-backed persistent memory (not Markdown)
    │   ├── context.rs         # Conversation context window management
    │   └── compaction.rs      # Truncate (NoopAdapter) or summarize (Phase 2a AI)
    ├── identity/
    │   ├── mod.rs
    │   └── bot_id.rs          # BotId: Ed25519 keypair + HKDF pseudonym
    ├── orchestrator/          # Phase 2a: recursive sub-agents
    │   ├── mod.rs
    │   ├── recursive.rs       # Recursive pipeline spawning (Clicky pattern)
    │   ├── subagent.rs        # Spawn child agent with scoped capabilities
    │   └── coordinator.rs     # Multi-bot coordination via shared state
    └── bridge/                # Phase 2a+: platform-specific bot bridges
        ├── mod.rs
        ├── rekindle.rs        # Phase 3: Rekindle community member bridge
        └── atproto.rs         # Phase 2a: ATProto bot bridge (malwarevangelist-derived)
```

### 14.3 Recursive Pipeline Orchestration

Derived from Clicky's subagent pattern. A pipeline stage can spawn a child
pipeline, which can spawn further children. This creates a recursive execution
tree with deterministic resource bounds at every level.

**Non-AI example (the default mode):**

```
RootPipeline: "Kick stream → Discord + Bluesky announce"
├── Stage 1: Trigger (ConnectorEvent: kick.stream_live)
├── Stage 2: Condition check (category == "gaming")
├── Stage 3: Data transform (extract title, username, URL)
└── Stage 4: Action dispatch (parallel children)
    ├── ChildPipeline A (fuel: parent/4)
    │   └── connector-discord: send_message → #announcements
    └── ChildPipeline B (fuel: parent/4)
        └── connector-bluesky: create_post → stream summary
```

No AI adapter involved. No LLM call. The rule engine evaluated a condition,
the transform stage formatted the output, and two connectors dispatched messages.
This is Springtale's primary operating mode.

**AI-enhanced example (opt-in per rule):**

```
RootPipeline: "PR review with optional AI analysis"
├── Stage 1: Trigger (ConnectorEvent: github.pull_request_opened)
├── Stage 2: Data collection
│   └── ChildPipeline (fuel: parent/4)
│       └── connector-github: get_diff
├── Stage 3: AI analysis (AiComplete, optional — skipped if NoopAdapter)
└── Stage 4: Action dispatch
    └── connector-github: post_comment (AI output OR raw diff)
```

If `AiComplete` encounters `NoopAdapter`, it passes through `prev.output`
unchanged. The pipeline continues. The comment contains the raw diff instead
of an AI analysis. **The workflow completes either way.**

**Resource isolation:** Each child pipeline receives a fraction of the parent's
fuel budget. Children cannot exceed parent bounds. The recursive depth is
configurable (default: 4 levels). This prevents unbounded recursion while
enabling complex multi-step workflows.

**State sharing:** Child pipelines inherit a read-only snapshot of the parent's
`PipelineContext`. Children write results to their own context. The parent
collects child results at the spawn-point stage. No mutable shared state
between concurrent children.

**Component security audit for `springtale-bot`:**

| Concern | Control |
|---|---|
| Subagent fuel escalation via race condition | Fuel budget is set at spawn time from parent's remaining fuel. Atomic decrement. Child cannot observe or modify parent's fuel counter. |
| Memory compaction AI injection | Compaction summarizes via AI adapter, but output is validated against typed schema before storage. If `NoopAdapter`, compaction uses truncation (keep most recent N entries) instead of summarization. |
| Bot-to-bot message spoofing | Inter-bot messages signed with sender's `AgentIdentity` Ed25519 key. Receiver verifies before processing. |
| Subagent inheriting parent's full capabilities | Children inherit a read-only capability snapshot. Children cannot request new capabilities. Children cannot elevate autonomy level. |
| Runaway orchestrator spawning unlimited children | Max concurrent children per pipeline: 8 (configurable). Max recursive depth: 4. Both enforced in `orchestrator::recursive`. |

**Component privacy audit for `springtale-bot`:**

| Concern | Control |
|---|---|
| Bot memory contains conversation history | Encrypted at rest (SQLite + XChaCha20-Poly1305 via `springtale-crypto` vault). Memory provenance tracks who wrote each entry (§15.5). `springtale-cli memory audit` for user inspection. |
| Bot sends PII to AI provider | `AiRequest` is a closed enum — only typed fields cross the AI boundary. `Secret<T>` values never serialize into AI requests. The user opts into each AI provider explicitly. |
| Bot's Rekindle DM history accessible to connectors | Connector sandbox cannot read bot memory. Memory access is through `springtale-bot` API only, not exposed as a WASI host function. |
| Bot logs may contain conversation content | `tracing` subscriber with `Secret<T>` redaction. Conversation content logged at TRACE level only (disabled in production). Event log stores trigger/action metadata, not payload content. |

### 14.4 Rekindle Bot Bridge (Phase 3)

When VeilidTransport is active, `springtale-bot` can join Rekindle communities:

```rust
// crates/springtale-bot/src/bridge/rekindle.rs

pub struct RekindleBotBridge {
    veilid_node: VeilidNode,
    community_key: TypedKey,
    slot_seed: SlotSeed,
    my_slot: u8,
    my_keypair: Ed25519Keypair,  // derived from slot_seed + my_slot
}

impl RekindleBotBridge {
    /// Join a community using invite secrets (same flow as human join).
    pub async fn join(invite: InviteSecrets) -> Result<Self>;

    /// Listen for channel messages. Calls handler on each new message.
    pub async fn on_message(&self, channel_id: ChannelId, handler: impl Fn(Message));

    /// Write a message to the bot's own subkey in the channel record.
    pub async fn send(&self, channel_id: ChannelId, content: &str) -> Result<()>;

    /// Listen for governance changes (channel CRUD, role assignments, bans).
    pub async fn on_governance_change(&self, handler: impl Fn(GovernanceEntry));

    /// Listen for member join/leave events via registry inspection.
    pub async fn on_member_change(&self, handler: impl Fn(MemberEvent));
}
```

This maps directly to the Rekindle Bot SDK described in §31 of the Rekindle
architecture. The bot writes to its own SMPL subkey, reads all other subkeys,
and merges governance entries using the same deterministic CRDT rules as
every other client. Permission enforcement is client-side: the reader
validates, not the writer. A rogue bot's entries are ignored by every
honest reader.

### 14.5 ATProto Bot Bridge

Derived from `malwarevangelist-bot`. Provides Bluesky bot capabilities
through the connector framework:

```rust
// crates/springtale-bot/src/bridge/atproto.rs

pub struct ATProtoBotBridge {
    session: AtpSession,         // managed by connector-bluesky auth module
    firehose: JetstreamConsumer, // WebSocket to Jetstream endpoint
}

impl ATProtoBotBridge {
    pub async fn on_mention(&self, handler: impl Fn(Mention));
    pub async fn on_follow(&self, handler: impl Fn(Follow));
    pub async fn post(&self, content: &str) -> Result<StrongRef>;
    pub async fn reply(&self, parent: &StrongRef, content: &str) -> Result<StrongRef>;
}
```

---


## 15. Proactive Architecture

Structural decisions that prevent entire classes of attacks by design.
For compliance mappings (OWASP ASVS, MITRE ATT&CK/ATLAS, Tanya Janca MVS),
competitive analysis, CI pipeline specs, and supply chain audit, see
[SECURITY.md](SECURITY.md).

### 15.1 Least Agency & Autonomy Levels

Based on AWS Agentic Security Scoping Matrix and CSA Autonomy Levels framework.
Autonomy is a design decision, not a capability. A capable agent can operate at
any autonomy level based on configuration.

**Five autonomy levels (configured per agent, per connector, per action):**

| Level | Name | Behavior | Human Role | Springtale Implementation |
|---|---|---|---|---|
| L0 | Observe | Read-only. Agent can query data but cannot take any action. | Operator | Connector manifest: `actions: []` (triggers only). Pipeline output is informational. |
| L1 | Suggest | Agent proposes actions but does not execute. All actions queued for human approval. | Approver | Pipeline pauses at `Handle` stage. Action queued in `springtale-store`. User approves/rejects via Tauri modal or dashboard. |
| L2 | Act-with-Approval | Agent executes approved plans. User approves a plan (batch of actions), agent executes within scope. | Collaborator | Pipeline generates plan → user reviews plan → approved plan executes autonomously. Deviations from plan require re-approval. |
| L3 | Act-Autonomously | Agent executes without per-action approval. User reviews outcomes asynchronously. Circuit breakers and sentinel active. | Consultant | Full pipeline execution. `springtale-sentinel` monitors. User reviews event log. Destructive actions still require approval (cannot be overridden to L3). |
| L4 | Self-Direct | Agent initiates actions proactively (heartbeat, cron). No human initiation required. Highest monitoring intensity. | Observer | Heartbeat + cron trigger pipelines. `springtale-sentinel` at maximum sensitivity. All actions logged with full context. Dead-man's switch active. |

**Default:** L1 (Suggest) for all new agents and connectors. Autonomy must be
explicitly elevated per-agent via `springtale-cli agent set-autonomy <name> <level>`.
The level cannot be set higher than the connector's manifest-declared maximum.

**Destructive actions are always L1 regardless of agent autonomy level.**
A connector action tagged `impact: destructive` (file delete, email send, account
modification, financial transaction) always pauses for human approval. This is
enforced in `springtale-core` pipeline engine, not in the connector — the connector
cannot override it.

### 15.2 `springtale-sentinel` — Runtime Behavioral Monitor

Phase 2a. Ships with hard constraints only (Layer 1). This covers the most
critical protections with zero false positives and no learning period needed.

```
crates/springtale-sentinel/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── rate_limiter.rs       # Actions/minute per connector, configurable
    ├── circuit_breaker.rs    # Per-stage failure counting, auto-disable + cooldown
    ├── dead_man.rs           # Actions/minute without user interaction → pause all
    ├── toxic_pairs.rs        # Dangerous capability combinations blocked at install
    ├── impact.rs             # Action impact classification (read-only / reversible / destructive)
    └── audit/
        ├── trail.rs          # Append-only audit trail (SQLite)
        └── export.rs         # Export for review via CLI
```

**Layer 1 controls (what ships):**

| Control | How it works |
|---|---|
| Rate limiter | Configurable actions/minute per connector. Default: 60. Exceed → throttle. |
| Circuit breaker | 3 consecutive stage failures → stage disabled, user notified. Auto-reset after cooldown. |
| Dead-man switch | > N actions/minute without user interaction → all pipelines pause. |
| Toxic pair blocker | `KeychainRead + NetworkOutbound(different host)` blocked at install time. |
| Destructive action gate | Actions tagged `impact: destructive` always require user approval. |
| Audit trail | Every action logged to append-only SQLite table. Exportable via CLI. |

**Deferred (not in Phase 2a):**
- Statistical baseline learning — requires defining "normal" per bot, varies by use case
- Trajectory analysis — research-grade anomaly detection on action sequences
- Layer 1 alone provides more protection than OpenClaw's zero monitoring

### 15.3 Toxic Combination Policy Engine

Certain capability pairs create attack paths even when each capability is
individually safe. The policy engine blocks these at connector install time.

| Combination | Risk | Policy |
|---|---|---|
| `KeychainRead` + `NetworkOutbound` (different host) | Credential exfiltration | Blocked unless same host (e.g., read OAuth token → send to that service's API) |
| `FilesystemRead` (broad path) + `NetworkOutbound` | Data exfiltration | Blocked. Filesystem connectors cannot have outbound network. |
| `ShellExec` + `NetworkOutbound` | RCE + C2 beacon | Blocked. Shell connectors are isolated from network. |
| `BrowserNavigate` + `KeychainRead` | Credential phishing via browser | Blocked. Browser connector uses session tokens, not keychain. |
| `FilesystemWrite` + `ShellExec` | Dropper pattern (write malware → execute) | Blocked. Write-capable connectors cannot execute. |

Custom rules can be added via `springtale.toml` configuration. The engine
evaluates at install time (reject manifest) and at runtime (reject capability
token request).

### 15.4 Agent Identity Layer

Agents are not users. They get their own cryptographic identity.

```rust
// crates/springtale-crypto/src/identity/agent_identity.rs

pub struct AgentIdentity {
    keypair: Secret<Ed25519Keypair>,   // unique per agent instance
    agent_id: AgentId,                 // derived from public key
    creator: UserId,                   // which user created this agent
    autonomy_level: AutonomyLevel,     // L0-L4
    max_autonomy: AutonomyLevel,       // ceiling from connector manifest
    created_at: DateTime<Utc>,
    credential_expiry: Duration,       // short-lived credentials, default 1 hour
}
```

**Key properties:**
- Agent identity is separate from user identity. Agents never inherit user
  sessions, cookies, or cached OAuth tokens.
- Credential scope is per-connector, per-task. A `CapabilityToken` is issued
  for each connector invocation and expires after task completion (default 1hr).
- Agent identity is logged in every audit trail entry. Actions are attributable
  to a specific agent, not just "the system."
- Phase 3: Agent identity maps to Rekindle `BotId` with HKDF pseudonym derivation
  for cross-community unlinkability.

### 15.5 Memory Provenance & Integrity

Every memory write records full provenance. Memory is not a dumb key-value store.

```rust
// crates/springtale-bot/src/memory/provenance.rs

pub struct MemoryEntry {
    id: MemoryId,
    content: EncryptedBlob,            // XChaCha20-Poly1305 encrypted
    schema_version: u32,               // typed schema, not arbitrary strings
    author: MemoryAuthor,              // User | Agent(AgentId) | Connector(ConnectorName)
    source: MemorySource,              // UserInput | ConnectorOutput | AiGenerated | Compacted
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>, // TTL for unverified data
    content_hash: [u8; 32],            // SHA-256 for integrity verification
    parent_version: Option<MemoryId>,  // for versioning/rollback
    trust_score: f32,                  // 1.0 = user-verified, 0.5 = ai-generated, 0.1 = unverified external
}
```

**Integrity guarantees:**
- **Write scanning:** New entries validated against typed schema before commit.
  Arbitrary strings rejected. Structured data only.
- **Provenance tracking:** Every entry records who wrote it, when, and from what
  source. AI-generated entries tagged as such and down-weighted on re-read.
- **No self-reinforcing loops:** Agent's own output is tagged `source: AiGenerated`.
  When read back, these entries have lower `trust_score` than user-verified data.
  Prevents the Palo Alto "stateful delayed-execution" attack.
- **Versioned snapshots:** `springtale-cli memory snapshot` creates immutable
  snapshots. `springtale-cli memory rollback <version>` restores to any snapshot.
- **Expiry:** Entries from unverified external sources (connector output, scraped
  web data) expire after configurable TTL (default 7 days). User-verified data
  does not expire.
- **Segmentation:** Memory is segmented per-connector and per-conversation.
  A connector's memory writes are isolated from other connectors' reads unless
  explicitly shared via typed pipeline context.

### 15.6 Data Lifecycle Management

```
Collection                Storage                  Processing               Retention                Deletion
    │                         │                         │                        │                       │
    ▼                         ▼                         ▼                        ▼                       ▼
Connector declares       Vault (secrets) or        WASM sandbox or          TTL policy per           springtale-cli
what data it collects    SQLite (structured)   pipeline stage           data source:             data purge
in manifest              SQLite (memory)           (capability-scoped)      • user: indefinite       --connector <name>
(required field)         All encrypted at rest     AI adapter: closed       • AI-generated: 30d     OR
User approves at         Secret<T> + zeroize       enum only, no raw        • external: 7d          data purge --all
install time             for in-memory secrets     data crosses boundary    • connector: configurable
```

**CLI commands for data sovereignty:**
- `springtale-cli data inventory` — list all data collected, per connector, with purpose
- `springtale-cli data export --format json` — export all user data (GDPR Article 15)
- `springtale-cli data purge --connector <name>` — delete all data from one connector
- `springtale-cli data purge --all` — nuclear option, delete everything
- `springtale-cli data retention set --source external --ttl 7d` — configure retention policy
- `springtale-cli memory snapshot` — create immutable memory snapshot
- `springtale-cli memory rollback <version>` — restore memory to snapshot
- `springtale-cli memory audit` — inspect all memory entries with provenance

---

---

*End of Architecture Document*
