# Phase 2a — Full Chat Coverage + AI Adapters

> Source: `docs/current-arch/ARCHITECTURE.md` §3, §6.7, §6.9-6.10, §14.3-14.5
> Depends on: Phase 1b complete

## Goal

Full chat platform coverage. AI adapters as optional fallback parser and
pipeline action. NL->Rule parser. Runtime behavioral monitoring. Sub-agent
orchestration. This is where Springtale becomes a full, safe replacement
for the existing agent/bot platforms.

## Transport Upgrade

Phase 2a upgrades from `LocalTransport` (Unix sockets, same-machine) to
`HttpTransport` (LAN/VPN, axum server + reqwest client, mTLS).

**How to integrate:**
- Implement `HttpTransport` in `crates/springtale-transport/src/http/`
- `http::server` — axum inbound endpoint, mTLS with rustls
- `http::client` — reqwest outbound, peer cert validation
- `runtime::boot` in `springtaled` selects transport based on `springtale.toml` config
- All existing code uses `Arc<dyn Transport>` — no changes needed outside transport crate
- mTLS cert fingerprints stored in `springtale-store`

**Research needed:** mTLS setup with rustls + reqwest. Peer certificate
pinning pattern. DNS rebinding protection via Host header validation.

## AI Adapters

The `AiAdapter` trait (defined in Phase 1a with NoopAdapter only) gets
real implementations. These are thin HTTP clients to user-provided endpoints.

**How to integrate:**
- Each adapter is a feature-gated module in `crates/springtale-ai/src/`
- `ollama::adapter` — HTTP client to `localhost:11434`, Ollama REST API
- `openai::adapter` — `/v1/chat/completions` for any OpenAI-compatible API (GPT, Gemini, Kimi, DeepSeek, OpenRouter)
- `anthropic::adapter` — `/v1/messages` with native tool_use
- `voice::stt` — Whisper-compatible speech-to-text bridge
- `voice::tts` — ElevenLabs / Piper text-to-speech bridge
- User configures endpoint in `springtale.toml` or `springtale-cli ai configure`
- HTTPS validated (HTTP rejected unless `--allow-insecure-ai-endpoint`)
- `AiOptions { max_tokens, timeout }` controls cost. Default: 4096 tokens, 30s
- Response body 10 MiB limit
- `AiRequest` is a closed enum — `Secret<T>` values cannot serialize into it

**Research needed:** Ollama REST API schema. OpenAI /v1/chat/completions
streaming SSE format. Anthropic /v1/messages tool_use format. Whisper API
variants (OpenAI, local faster-whisper).

## NL->Rule Parser

User says "notify me on Discord when my Kick stream goes live" -> AI generates
structured `Rule` (TOML) -> rule runs deterministically forever, no AI needed again.

**How to integrate:**
- `springtale-ai::parser::rule_gen` — prompt templates inject available connector schemas
- `springtale-ai::parser::prompt` — structured output prompt for the user's AI
- Output is a `Rule` struct (from springtale-core), validated before saving
- User reviews generated TOML before enabling
- Works with any AI adapter (Ollama, OpenAI, Anthropic)

## Chat Connectors (7)

Each follows `.claude/rules/connector-guidelines.md`.

| Connector | Key Integration Notes |
|---|---|
| `connector-discord` | Gateway WebSocket (not REST polling). Slash command registration. Voice channel join for presence. Embed builder. discord.js-equivalent in Rust. |
| `connector-signal` | signal-cli bridge process. E2E encrypted messages. Disappearing messages support. No phone number exposed to Springtale. |
| `connector-whatsapp` | Baileys-equivalent protocol. **Likely NativeConnector** (needs persistent WebSocket + Signal Protocol — too complex for WASM sandbox). QR code pairing. |
| `connector-matrix` | Matrix SDK (ruma crate). Federated rooms. E2E encryption via vodozemac. Room state management. |
| `connector-irc` | Lightweight. Raw TCP + rustls-tls. Channel join/part/msg. SASL auth. |
| `connector-slack` | Socket mode (not HTTP). Slash commands. Block Kit. Thread replies. |
| `connector-nostr` | NIP-01 relay client. Event signing with Ed25519. Encrypted DMs (NIP-04/NIP-44). |

**Research needed per connector:** API docs, auth flows, WebSocket vs REST,
rate limits, message format. connector-whatsapp needs special attention —
Baileys reverse-engineers WhatsApp's protocol and may need NativeConnector
trust level rather than WASM sandbox.

## springtale-sentinel

Runtime behavioral monitor. Ships with hard constraints only (Layer 1).

**How to integrate:**
- New crate: `crates/springtale-sentinel/`
- Sentinel instance created in `springtaled` startup, passed to bot runtime
- Bot runtime wraps pipeline dispatch: `sentinel.evaluate(action, connector)` before each stage
- Sentinel does NOT live in springtale-core (no dependency cycle)
- Integration point is `springtaled` and `springtale-bot` event loop

**Modules:**
- `rate_limiter` — actions/minute per connector, default 60, configurable
- `circuit_breaker` — 3 consecutive failures -> stage disabled, user notified, auto-reset after cooldown
- `dead_man` — N actions/minute without user interaction -> pause all pipelines
- `toxic_pairs` — dangerous capability combos blocked at install time
- `impact` — action tagged read-only / reversible / destructive
- `audit::trail` — append-only SQLite table, every action logged
- `audit::export` — export for CLI review + compliance

## Recursive Pipeline Orchestration

Derived from Clicky's subagent pattern.

**How to integrate:**
- `springtale-bot::orchestrator::recursive` — pipeline stage can spawn child pipeline
- Each child gets `parent_remaining_fuel / num_children` fuel budget
- Max concurrent children: 8. Max recursive depth: 4.
- Children inherit read-only `PipelineContext` snapshot. Write to own context.
- Parent collects child results at spawn-point stage.
- `springtale-bot::orchestrator::subagent` — spawn child agent with scoped capabilities
- `springtale-bot::orchestrator::coordinator` — multi-bot coordination via shared state

## ATProto Bot Bridge

Derived from malwarevangelist-bot.

**How to integrate:**
- `springtale-bot::bridge::atproto` — wraps connector-bluesky triggers/actions
- `ATProtoBotBridge` provides `on_mention`, `on_follow`, `post`, `reply`
- Maps Bluesky events to bot event loop
- Session management patterns from malwarevangelist-bot's enCore engine

## Memory-Only / Ephemeral Mode

`--ephemeral` flag: vault exists only in memory. All state lost on exit.

**How to integrate:**
- `springtale-store::backend::memory` — in-memory StorageBackend implementation
- `springtale-crypto::vault` — option to skip file persistence
- `springtaled --ephemeral` creates everything in memory, warns user on startup

## Not In Phase 2a

- No Tauri desktop/mobile shell (2b)
- No visual rule builder (2b)
- No dashboard web UI (2b)
- No browser automation connector (2b)
- No Veilid transport (Phase 3)
- No Rekindle bot bridge (Phase 3)
- No statistical baseline learning for sentinel (deferred)
- No trajectory analysis for sentinel (deferred)
