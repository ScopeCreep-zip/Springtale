# Glossary

Terms used throughout Springtale's codebase and documentation. Each entry links to where the concept appears in the project.

```
   Application                  Bot                     Runtime ops
   ┌─────────┐          ┌────────────────┐       ┌──────────────────┐
   │springtaled         │ event loop     │       │ dispatch_action  │
   │springtale-cli      │ router         │       │ sentinel         │
   │Tauri desktop/web   │ cooperation    │       │ operations/*     │
   └────┬───────┘       └────────┬───────┘       └────────┬─────────┘
        │                        │                        │
        ▼                        ▼                        ▼
   ┌──────────────────────────────────────────────────────────────┐
   │  Foundation                                                  │
   │                                                              │
   │  core     crypto    store     transport    scheduler         │
   │  rules    vault     SQLite    Local/HTTP   cron/watcher      │
   │  pipeline KDF/AEAD  migrs     mTLS         jobs/retry        │
   │                                                              │
   │  ai       mcp       connector  sentinel                      │
   │  Noop/…   rmcp      trait+WASM monitor                       │
   └──────────────────────────────────────────────────────────────┘
                                │
                                ▼
                      ┌──────────────────┐
                      │  14 connectors   │
                      │  (native Rust)   │
                      └──────────────────┘
```

*Fig. 1. Where each term in this glossary fits.*

---

## A

**Action** — What a rule does when its trigger fires and conditions pass. Types include `RunConnector`, `SendMessage`, `WriteFile`, `RunShell`, `Chain`, `Transform`, `Delay`, and `AiComplete`. Defined in `crates/springtale-core/src/rule/types.rs`. See [guide/rules.md](guide/rules.md).

**Argon2id** — Memory-hard key derivation function used to derive encryption keys from passphrases. Springtale uses it in the vault to protect stored secrets. Implemented in `crates/springtale-crypto/src/vault/`.

**ATProto** — The AT Protocol, Bluesky's federated social networking protocol. `connector-bluesky` authenticates via ATProto session tokens and subscribes to events via Jetstream. See [reference/connectors/bluesky.md](reference/connectors/bluesky.md).

**Async trait** — Rust doesn't natively support `async fn` in traits (stabilized but not yet ubiquitous). Springtale uses the `async-trait` crate to define async methods on traits like `Connector`, `Transport`, `AiAdapter`, and `Stage`.

## B

**Blackboard** — Shared key/value workspace inside a formation. Members post and read data without sending direct messages, enabling stigmergic coordination (act-by-leaving-traces). Implemented in `crates/springtale-bot/src/cooperation/blackboard/` (live store, write log) and `crates/springtale-cooperation/src/state/` (types and semantics). Each tick's writes are split by Lamport-style `last_tick_write_count` for interference detection in step 2 of the tick pipeline.

**Bot** — A Springtale agent with a command router, persona, session memory, and access to connectors. Lives in `crates/springtale-bot`. Bots join chat platforms through chat connectors (Telegram, Discord, IRC, etc.) and coordinate with peers via the **cooperation** framework.

## C

**Cadence** — The shared tick bus that coordinates formation members without central control. Implemented as a `tokio::sync::broadcast` channel in `crates/springtale-cooperation/src/cadence.rs`. Emits `Tick { sequence, timestamp, intent }` at a configurable interval. Slow consumers drop to a lagged signal rather than blocking the bus.

**Canvas** — The live pixel-art visualisation of running connectors, rules, agents, and formations rendered by the Tauri desktop app and web dashboard. Connectors are nodes; rules and agents are springtails; formations are zones; pipelines are mycelium. State comes from `CanvasState` in `springtale-core::canvas`, delta updates stream over `/canvas/stream` (SSE). Live formation detail is read through the `LiveFormationReader` trait in `springtale-runtime`. See [guide/colony-canvas.md](guide/colony-canvas.md).

**Chiral diorama** — A Tauri desktop theme inspired by Death Stranding's diorama aesthetic. Default theme as of April 2026. Coexists with the original colony forest theme. Frontend-only — theme selection does not affect backend behaviour.

**Capability** — A declared permission that a connector requires to function. Springtale enforces capabilities at install time and again at every action invocation. Variants: `NetworkOutbound { host }` (no wildcards), `FilesystemRead { path }`, `FilesystemWrite { path }`, `KeychainRead { key }`, `ShellExec` (always requires explicit approval). Defined in `crates/springtale-connector/src/manifest/types.rs`. See [guide/connectors.md](guide/connectors.md).

**Cooperation framework** — The 37-module crate `crates/springtale-cooperation/` (extracted from `springtale-bot` in April 2026) that implements non-hierarchical multi-agent coordination: cadence, momentum, formations, shared environment, orchestrator gating, attention economy, rally, sacrifice, recovery, supervision, stigmergy, contract net, routing, mental model, role dynamics, handoff, pacing, consensus, commit barriers, interference, transformation, and more. Wired into `springtale-bot` through a 14-step formation tick in `springtale-bot::runtime::event_loop::handle_cadence_tick`. Designed against the spec in [`docs/intended-arch/COOPERATION.md`](intended-arch/COOPERATION.md); user-facing tour in [`docs/guide/cooperation.md`](guide/cooperation.md).

**Cooperation crate (`springtale-cooperation`)** — Standalone crate with zero internal Springtale dependencies. Depended on by `springtale-bot`, `springtale-runtime`, and `springtale-store`. Holds all cooperation types, traits, and algorithms. Game-informed — draws directly from Spring RTS, RimWorld ThinkTree, Erlang OTP supervision, Kubernetes liveness probes, L4D AI Director, Monster Hunter rally mechanics, Overcooked implicit signalling, Microsoft AGT momentum, 0 A.D. stance systems. See [guide/cooperation.md](guide/cooperation.md).

**Condition** — A filter on a rule that must evaluate to `true` before actions fire. Supports `And`, `Or`, `Not`, `FieldEquals`, `Contains`, `Regex`, `TimeInRange`, and `DayOfWeek`. See [guide/rules.md](guide/rules.md).

**Connector** — An adapter between Springtale and an external service (Kick, GitHub, Bluesky, etc.) or local resource (filesystem, shell). Connectors declare triggers they emit and actions they can perform. See [guide/connectors.md](guide/connectors.md).

**Contract Net Protocol (CNP)** — FIPA-standard task allocation pattern. A task-holder broadcasts a Call-for-Proposals, peers submit bids, the holder awards one. Implemented in `crates/springtale-cooperation/src/contract_net/`. Used for ad-hoc work distribution beyond the blackboard's passive pull model.

**CRDT** — Conflict-free Replicated Data Type. Used in the Rekindle protocol (Phase 3) for eventually-consistent governance and message ordering without a central server. See `docs/current-arch/rekindle-architecture.md`.

## D

**DHT** — Distributed Hash Table. Veilid's storage layer. Springtale's `VeilidTransport` (currently a stub — every method returns `TransportError::NotConnected`) will use it for the connector registry and Rekindle community records. Each record uses SMPL subkeys with 255 writer slots.

**Duress passphrase** — A secondary passphrase that unlocks a decoy vault instead of the real one. Implemented via two AEAD-encrypted regions in a single vault file with a constant 131,152-byte size (padding prevents traffic analysis). Writing one region preserves the other byte-for-byte. Configure via `springtale vault duress-setup`. See `crates/springtale-crypto/src/vault/duress.rs`.

## E

**Ed25519** — An elliptic curve digital signature algorithm. Springtale uses Ed25519 for node identity keypairs, manifest signing, and capability token signatures. Implemented via the `ed25519-dalek` crate in `crates/springtale-crypto/`.

## F

**Formation** — A peer group of bot agents cooperating on a shared intent. No hierarchy — members are siblings. Defined in `crates/springtale-bot/src/cooperation/formation.rs` (live struct + members) and `crates/springtale-cooperation/src/context.rs` (`FormationContext` shared read-only state). Has an `intent`, `momentum` tier, shared `blackboard` environment, rally tokens, attention broker, gossip-store-backed awareness, mental model, pacing state, and optional orchestrator (AI-gated at Fever tier).

**Formation command** — A user-triggered or runtime-originated lifecycle transition: `Deploy`, `Pause`, `Resume`, `Dissolve`, `ChangeIntent`, `AddMember`, `RemoveMember`, `Rally`. Defined in `crates/springtale-cooperation/src/command.rs`. Delivered to the bot event loop via an mpsc channel; the bot is the only code path that materialises live `Formation` structs from DB rows or removes them. Surfaces in the UI as the formation command grid.

**Formation intent pattern** — A high-level goal string (e.g. "reconnoiter Telegram and Nostr for harassment patterns") stored on a formation. Typed via `IntentPattern` in `crates/springtale-cooperation/src/cadence.rs`. When a formation reaches Fever momentum, the orchestrator decomposes the intent into sub-tasks using the AI adapter.

**Fuel metering** — Wasmtime's mechanism for limiting how many instructions a WASM module can execute. Springtale gives WASM connectors a budget of 10 million instructions per invocation. Exceeding the budget terminates execution. Configured in `crates/springtale-connector/src/wasm/runtime.rs`.

## G

**Guard status** — Per-formation toggle that gates destructive or high-impact actions on members (e.g. dissolve, force-rally, intent change). Surfaces in the colony canvas formation detail card as a badge and is toggled via `POST /formations/{id}/toggle-guard`. State lives on the live `Formation` struct and is broadcast through `LiveFormationReader`. Read by the dashboard command grid to enable/disable destructive buttons.

## H

**Handoff** — A work-product transfer between agents. Five variants in `crates/springtale-cooperation/src/handoff/`: `Direct` (agent-to-agent), environment-mediated (deposit into shared workspace), `FlexChain` (flexible sequence), sequential, informational. Distinct from task dispatch — handoff moves the *result* of a completed sub-task, not the sub-task itself.

**HMAC** — Hash-based Message Authentication Code. Used for API bearer token generation (HMAC-SHA256 of passphrase) and webhook signature verification (GitHub uses HMAC-SHA256). See [reference/api.md](reference/api.md).

**HKDF** — HMAC-based Key Derivation Function. The `VeilidTransport` design uses HKDF to derive per-community pseudonyms from a single Ed25519 identity, preventing cross-community identity correlation. The transport is currently a stub.

## J

**Interference** — A conflict between two agents' actions in the same tick. Four kinds, detected in `crates/springtale-cooperation/src/interference/`: resource conflict (two agents claim the same resource), action negation (one action undoes another), collateral damage, redundancy. Detected in step 2 of the tick pipeline from the shared-env write log; feeds momentum updates in step 4.

**Jetstream** — Bluesky's real-time event firehose over WebSocket. `connector-bluesky` subscribes to Jetstream to receive mentions, follows, likes, and reposts. See [reference/connectors/bluesky.md](reference/connectors/bluesky.md).

## K

**KDF** — Key Derivation Function. A function that derives cryptographic keys from passwords or other key material. Springtale uses Argon2id as its KDF. See **Argon2id**.

**Keypair** — An Ed25519 public/private key pair that serves as a node's identity. Generated during `springtale init` and stored encrypted in the vault. See `crates/springtale-crypto/src/identity/`.

## L

**LiveFormationReader** — A trait in `crates/springtale-runtime/src/` that exposes enriched live formation state (momentum, rally tokens, attention load, guard status, member health/liveness, intent) to the Tauri IPC commands and the dashboard HTTP API. The bot event loop owns the live `Formation` structs; the reader is the one-way read channel out to UI code paths without exposing internal types.

## M

**Manifest** — A TOML file that declares a connector's metadata, capabilities, triggers, and actions. Manifests are Ed25519-signed and verified before loading. See [guide/connectors.md](guide/connectors.md).

**MCP** — Model Context Protocol. An open protocol for connecting AI models to tools and data sources. Springtale's `springtale-mcp` crate uses `rmcp` 1.x to automatically expose any connector as an MCP server via stdio. Capabilities are re-checked at both `list_tools` and `call_tool` — MCP does not bypass the sandbox. See [guide/connectors.md](guide/connectors.md).

**Mental model** — The shared cognitive state of a formation: domain knowledge, capability awareness, cooperation patterns, vocabulary, and conventions. Defined in `crates/springtale-cooperation/src/mental_model/`. Persisted across formation deploys in the `mental_model_*` tables (`crates/springtale-store/src/schema/sql/cooperation.sql`) so later formations with the same id benefit from what prior instances learned. Updated on step 13 of every tick via `mental_model::learning::update_model`.

**Momentum** — A formation's coherence state. Four tiers: Cold (just started) → Warming (≥3 successful ticks) → Hot (≥8 successes, no interference) → Fever (≥15 successes, no interference). Each tier unlocks runtime capabilities — Cold can read the shared environment, Hot can write and commit, Fever gets AI access, consensus voting, and orchestration. Defined in `crates/springtale-cooperation/src/momentum.rs`. Persisted to the `formation_momentum` table every tick.

## N

**Native connector** — A first-party connector compiled as Rust and loaded in-process. High trust, audited by the Springtale team. All 14 first-party connectors are native today. Contrast with **WASM connector**.

**NoopAdapter** — The default AI adapter that does nothing. Returns a fixed "no AI configured" response. Proves that the entire platform works without any AI plugged in. Defined in `crates/springtale-ai/src/noop/`.

## O

**OAuth 2.1 PKCE** — The authorization flow used by `connector-kick`. PKCE (Proof Key for Code Exchange) prevents authorization code interception attacks without requiring a client secret.

**Orchestrator** — The component in `springtale-bot` that decomposes a formation's intent into sub-tasks via an AI adapter. Only invoked when the formation reaches Fever momentum tier. Sub-tasks are posted to the formation's shared blackboard under `task:*` keys; members pull via the agent loop (`scan` step). Defined in `crates/springtale-bot/src/orchestrator/` (split into `composer`, `intervention`, `orchestrate`). The `intervention` module handles L6 escalation actions: `change_intent`, `dissolve`, `escalate`, `inject_fuel`.

**Rally** — A self-healing step a formation takes before escalating to the orchestrator. Implemented in `crates/springtale-cooperation/src/rally/`. Consumes a rally token, redirects attention to a weakest agent, and attempts to stabilise momentum. If tokens are exhausted, the formation escalates. Surfaces in the UI as rally pips (Monster Hunter-style cart icons).

**Recovery** — Distress-signal-driven mutual aid between agents. `crates/springtale-cooperation/src/recovery/`. Non-operational members emit `DistressSignal::HealthLow`, `Incapacitated`, or `Dead`; each operational peer runs `evaluate_recovery` to decide whether to help (first willing helper wins, per L4D: nearest survivor rescues pinned teammate). Runs as step 9b of the tick pipeline.

**OWASP ASVS** — The OWASP Application Security Verification Standard. Springtale targets Level 2 compliance. Mapping in `docs/current-arch/SECURITY.md`.

## P

**Pacing** — Work/rest phase control at the formation level. `crates/springtale-cooperation/src/pacing/`. GCRA-based rate limiter inspired by the L4D AI Director: detects when the formation has been in sustained high-pressure activity and transitions to a cooldown phase. Evaluated in step 8 of the tick pipeline.

**Panic wipe** — Single-pass random overwrite of vault key material followed by re-creation of an empty vault file. Triggered by entering the duress passphrase or running `springtale panic`. Completes in under 3 seconds on a 1 MB vault. Implemented in `crates/springtale-crypto/src/vault/wipe.rs`. Distinct from `data purge`, which deletes user data while keeping the vault.

**Pipeline** — A sequence of processing stages that transform data between trigger and action. Each stage reads from and writes to a `PipelineContext`. Stages compose left-to-right. See [guide/rules.md](guide/rules.md).

**PipelineContext** — The data bag that flows through pipeline stages. Contains input, output, errors, retry count, chain depth, and attachments. Defined in `crates/springtale-core/src/pipeline/`.

## R

**Rule** — The core automation unit: a trigger, zero or more conditions, and one or more actions. Rules are authored in TOML, stored in SQLite, and evaluated by the `RuleEngine`. See [guide/rules.md](guide/rules.md).

**RuleEngine** — Evaluates incoming trigger events against all enabled rules, returning matches with their actions. Pure evaluation — no side effects. Defined in `crates/springtale-core/src/rule/`.

**rmcp** — The Rust SDK for Model Context Protocol. Springtale pins `rmcp` 1.x and uses its stdio transport for the MCP bridge.

**rustls** — A TLS implementation written in pure Rust. Springtale uses rustls exclusively — `native-tls` and OpenSSL are banned at compile time via `deny.toml` and a vendor stub at `vendor/native-tls-stub/`.

## S

**Sacrifice** — A deliberate self-cost an agent pays for formation benefit (e.g. burning its own rally tokens to shield a weaker peer). `crates/springtale-cooperation/src/sacrifice/`. Evaluator snapshot feeds the recovery decision loop in step 9b.

**Sandbox (WASM)** — The Wasmtime isolation boundary for community connectors. Limits: 10M instruction fuel, 64MB memory (1024 pages), 30-second wall-clock timeout. Only declared capabilities are exposed via the host API. Each invocation gets a fresh `Store` — no cross-call state leakage.

**Sentinel** — The behavioural monitor in `crates/springtale-sentinel/`. Detects toxic capability pairs at manifest install time and writes audit entries to the `audit_trail` table. Always on — there is no "disable sentinel" mode.

**Secret\<T\>** — A wrapper type from the `secrecy` crate. Values inside cannot be logged, cloned, or accidentally serialized. Memory is zeroed on drop via `zeroize`. All credentials in Springtale are `Secret<String>`.

**SMPL** — A Veilid DHT record type that supports multiple writers, each assigned a subkey. Used in Rekindle for governance CRDTs and channel message storage. Maximum 255 writer slots.

**Stage** — A unit of processing in a pipeline. Implements the `Stage` trait with `name()` and `async call(ctx)`. Stages are composed via `compose_pipeline()`. See **Pipeline**.

**Stigmergy** — L0 ambient signalling between agents via shared environmental surfaces. `crates/springtale-cooperation/src/stigmergy/`. Inspired by ant pheromone trails: an agent marks a surface (e.g. "I handled this GitHub event"); other agents perceive the mark and adjust behaviour without direct messaging. Decays over time. Composes via tables of surfaces with typed signal keys.

**Subscription** — A first-class handle returned by `Connector::on_event()` and used to cancel an event subscription cleanly. Defined in `crates/springtale-connector/`. Every first-party connector adopted the `Subscription` lifecycle in April 2026; WASM host functions were extended to pass subscription IDs back and forth across the sandbox boundary. See [contributing/adding-a-connector.md](contributing/adding-a-connector.md).

**Supervision** — Agent lifecycle management combining Erlang OTP supervisor patterns, Kubernetes liveness probes, and a project `FAILURE.md` taxonomy. `crates/springtale-cooperation/src/supervision/`. Watches each member's liveness (`Alive` / `Suspect` / `Down`) and consecutive failure count; dispatches `TransformRole`, `RetryWithRally`, `TriggerReplan`, `MarkDown`, or `Escalate` actions. Runs as step 4d of the tick pipeline.

## T

**Tauri** — A framework for building desktop and mobile apps with web frontends and Rust backends. Springtale's desktop shell uses Tauri 2 with a SolidJS + Tailwind 4 frontend. The desktop app and the web dashboard share a common component library (`tauri/packages/ui`) with a `DataProvider` abstraction: the desktop wraps Tauri `invoke()`, the web wraps HTTP + SSE.

**Tool call** — A structured invocation emitted by an AI adapter when the AI wants to call a connector action. Typed as `ToolCall` in `springtale-ai`. The bot's `tool_runner` routes the call through the same capability gate used for direct actions — there is no back door. Supported by all three AI adapters (Anthropic, Ollama, OpenAI-compat); the MCP server exposes connectors as tools to external AI callers via the same path.

**Toxic pair** — A dangerous combination of capabilities that could enable data exfiltration. Example: `KeychainRead` + `NetworkOutbound` to a different host. Blocked at install time. See [guide/security.md](guide/security.md).

**Transport** — The abstraction for inter-node communication. Implementations: `LocalTransport` (Unix domain socket, present), `HttpTransport` (rustls mTLS, present), `VeilidTransport` (P2P — stub, every method returns `TransportError::NotConnected`). All implement the `Transport` trait. See [guide/architecture.md](guide/architecture.md).

**Trigger** — What kicks off a rule. Types: `Cron`, `FileWatch`, `Webhook`, `ConnectorEvent`, `SystemEvent`. One trigger per rule. See [guide/rules.md](guide/rules.md).

## V

**Vault** — An encrypted binary file (`vault.bin`) that stores keypairs and secrets. Encrypted with XChaCha20-Poly1305, key derived from passphrase via Argon2id (64 MiB memory, 3 iterations, 4 parallelism). The vault file has no magic bytes and is indistinguishable from random data without the passphrase. A configured duress passphrase adds a second encrypted region in the same file with a constant total size of 131,152 bytes. Created by `springtale init`. See [guide/security.md](guide/security.md).

**Veilid** — A privacy-focused peer-to-peer networking framework. Springtale's `VeilidTransport` (currently a stub) targets Veilid for encrypted P2P communication with no central server and no IP leakage.

## W

**WAL** — Write-Ahead Logging. SQLite's WAL mode allows concurrent readers and a single writer without blocking. Springtale enables WAL on its SQLite database for concurrency.

**WASM** — WebAssembly. A portable binary format. Community connectors compile to WASM and run inside a Wasmtime sandbox with strict resource limits.

**WASI** — WebAssembly System Interface. The standard for WASM modules to interact with the host system. Springtale targets WASI Preview 2 (`wasm32-wasip2`).

**WASM connector** — A community-authored connector compiled to WASM and executed in the Wasmtime sandbox. Low trust, untrusted by default. Contrast with **Native connector**.

**Wasmtime** — A WASM runtime from the Bytecode Alliance. Springtale uses Wasmtime for sandbox execution of community connectors, with fuel metering and memory limits.

## X

**XChaCha20-Poly1305** — An authenticated encryption cipher. Used for vault encryption (secrets at rest). XChaCha20 provides a 192-bit nonce, eliminating nonce-reuse concerns for long-lived keys.

## Z

**Zeroize** — The process of overwriting sensitive memory with zeros before deallocation. All `Secret<T>` values in Springtale implement `Zeroize` via the `zeroize` crate, preventing secret leakage through freed memory.

---

## References

- [1] As-built architecture: [`docs/arch/ARCHITECTURE.md`](arch/ARCHITECTURE.md)
- [2] As-built security: [`docs/arch/SECURITY.md`](arch/SECURITY.md)
- [3] Design intent: [`docs/current-arch/ARCHITECTURE.md`](current-arch/ARCHITECTURE.md)
- [4] Cooperation framework: [`docs/intended-arch/COOPERATION.md`](intended-arch/COOPERATION.md)
- [5] Rekindle Protocol: [`docs/current-arch/rekindle-architecture.md`](current-arch/rekindle-architecture.md)
- [6] secrecy crate: `https://docs.rs/secrecy`
- [7] Wasmtime: `https://wasmtime.dev`
- [8] Veilid: `https://veilid.com`
