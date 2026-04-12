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

**Bot** — A Springtale agent with a command router, persona, session memory, and access to connectors. Lives in `crates/springtale-bot`. Bots join chat platforms through chat connectors (Telegram, Discord, IRC, etc.) and coordinate with peers via the **cooperation** framework.

## C

**Cadence** — The shared tick bus that coordinates formation members without central control. Implemented as a `tokio::sync::broadcast` channel in `crates/springtale-bot/src/cooperation/cadence.rs`. Emits `Tick { sequence, timestamp, intent }` at a configurable interval. Slow consumers drop to a lagged signal rather than blocking the bus.

**Canvas** — The live pixel-art visualisation of running connectors, rules, agents, and formations rendered by the Tauri desktop app and web dashboard. Connectors are trees; rules and agents are springtails; formations are zones; pipelines are mycelium. State comes from `CanvasState` in `springtale-core::canvas`, delta updates stream over `/canvas/stream` (SSE).

**Capability** — A declared permission that a connector requires to function. Springtale enforces capabilities at install time and again at every action invocation. Variants: `NetworkOutbound { host }` (no wildcards), `FilesystemRead { path }`, `FilesystemWrite { path }`, `KeychainRead { key }`, `ShellExec` (always requires explicit approval). Defined in `crates/springtale-connector/src/manifest/types.rs`. See [guide/connectors.md](guide/connectors.md).

**Cooperation framework** — The set of modules in `crates/springtale-bot/src/cooperation/` that implements non-hierarchical multi-agent coordination: cadence, momentum, formations, shared environment, orchestrator gating, attention economy, rally, sacrifice, and recovery. Designed against the spec in [`docs/intended-arch/COOPERATION.md`](intended-arch/COOPERATION.md).

**Condition** — A filter on a rule that must evaluate to `true` before actions fire. Supports `And`, `Or`, `Not`, `FieldEquals`, `Contains`, `Regex`, `TimeInRange`, and `DayOfWeek`. See [guide/rules.md](guide/rules.md).

**Connector** — An adapter between Springtale and an external service (Kick, GitHub, Bluesky, etc.) or local resource (filesystem, shell). Connectors declare triggers they emit and actions they can perform. See [guide/connectors.md](guide/connectors.md).

**CRDT** — Conflict-free Replicated Data Type. Used in the Rekindle protocol (Phase 3) for eventually-consistent governance and message ordering without a central server. See `docs/current-arch/rekindle-architecture.md`.

## D

**DHT** — Distributed Hash Table. Veilid's storage layer. Springtale's `VeilidTransport` (currently a stub — every method returns `TransportError::NotConnected`) will use it for the connector registry and Rekindle community records. Each record uses SMPL subkeys with 255 writer slots.

**Duress passphrase** — A secondary passphrase that unlocks a decoy vault instead of the real one. Implemented via two AEAD-encrypted regions in a single vault file with a constant 131,152-byte size (padding prevents traffic analysis). Writing one region preserves the other byte-for-byte. Configure via `springtale vault duress-setup`. See `crates/springtale-crypto/src/vault/duress.rs`.

## E

**Ed25519** — An elliptic curve digital signature algorithm. Springtale uses Ed25519 for node identity keypairs, manifest signing, and capability token signatures. Implemented via the `ed25519-dalek` crate in `crates/springtale-crypto/`.

## F

**Formation** — A peer group of bot agents cooperating on a shared intent. No hierarchy — members are siblings. Defined in `crates/springtale-bot/src/cooperation/formation.rs`. Has an `intent`, `momentum` tier, shared `environment` blackboard, and optional orchestrator (AI-gated).

**Fuel metering** — Wasmtime's mechanism for limiting how many instructions a WASM module can execute. Springtale gives WASM connectors a budget of 10 million instructions per invocation. Exceeding the budget terminates execution. Configured in `crates/springtale-connector/src/wasm/runtime.rs`.

## H

**HMAC** — Hash-based Message Authentication Code. Used for API bearer token generation (HMAC-SHA256 of passphrase) and webhook signature verification (GitHub uses HMAC-SHA256). See [reference/api.md](reference/api.md).

**HKDF** — HMAC-based Key Derivation Function. The `VeilidTransport` design uses HKDF to derive per-community pseudonyms from a single Ed25519 identity, preventing cross-community identity correlation. The transport is currently a stub.

## J

**Jetstream** — Bluesky's real-time event firehose over WebSocket. `connector-bluesky` subscribes to Jetstream to receive mentions, follows, likes, and reposts. See [reference/connectors/bluesky.md](reference/connectors/bluesky.md).

## K

**KDF** — Key Derivation Function. A function that derives cryptographic keys from passwords or other key material. Springtale uses Argon2id as its KDF. See **Argon2id**.

**Keypair** — An Ed25519 public/private key pair that serves as a node's identity. Generated during `springtale init` and stored encrypted in the vault. See `crates/springtale-crypto/src/identity/`.

## M

**Manifest** — A TOML file that declares a connector's metadata, capabilities, triggers, and actions. Manifests are Ed25519-signed and verified before loading. See [guide/connectors.md](guide/connectors.md).

**MCP** — Model Context Protocol. An open protocol for connecting AI models to tools and data sources. Springtale's `springtale-mcp` crate uses `rmcp` 1.x to automatically expose any connector as an MCP server via stdio. Capabilities are re-checked at both `list_tools` and `call_tool` — MCP does not bypass the sandbox. See [guide/connectors.md](guide/connectors.md).

**Mental model** — The shared cognitive state of a formation: domain knowledge, capability awareness, cooperation patterns, vocabulary, and conventions. Defined in `crates/springtale-bot/src/cooperation/mental_model.rs`. Currently a type definition; behavioural integration is pending (see [`docs/arch/AUDIT-NOTES.md`](arch/AUDIT-NOTES.md)).

**Momentum** — A formation's coherence state. Four tiers: Cold (just started) → Warming (≥3 successful ticks) → Hot (≥8 successes, no interference) → Fever (≥15 successes, no interference). Each tier unlocks runtime capabilities — Cold can read the shared environment, Hot can write and commit, Fever gets AI access and consensus voting. Defined in `crates/springtale-bot/src/cooperation/momentum.rs`.

## N

**Native connector** — A first-party connector compiled as Rust and loaded in-process. High trust, audited by the Springtale team. All 14 first-party connectors are native today. Contrast with **WASM connector**.

**NoopAdapter** — The default AI adapter that does nothing. Returns a fixed "no AI configured" response. Proves that the entire platform works without any AI plugged in. Defined in `crates/springtale-ai/src/noop/`.

## O

**OAuth 2.1 PKCE** — The authorization flow used by `connector-kick`. PKCE (Proof Key for Code Exchange) prevents authorization code interception attacks without requiring a client secret.

**Orchestrator** — The component in `springtale-bot` that decomposes a formation's intent into subtasks via an AI adapter. Only invoked when the formation reaches Fever momentum tier. Subtasks are posted to the formation's shared blackboard; members pull them. Defined in `crates/springtale-bot/src/orchestrator/`.

**OWASP ASVS** — The OWASP Application Security Verification Standard. Springtale targets Level 2 compliance. Mapping in `docs/current-arch/SECURITY.md`.

## P

**Pipeline** — A sequence of processing stages that transform data between trigger and action. Each stage reads from and writes to a `PipelineContext`. Stages compose left-to-right. See [guide/rules.md](guide/rules.md).

**PipelineContext** — The data bag that flows through pipeline stages. Contains input, output, errors, retry count, chain depth, and attachments. Defined in `crates/springtale-core/src/pipeline/`.

## R

**Rule** — The core automation unit: a trigger, zero or more conditions, and one or more actions. Rules are authored in TOML, stored in SQLite, and evaluated by the `RuleEngine`. See [guide/rules.md](guide/rules.md).

**RuleEngine** — Evaluates incoming trigger events against all enabled rules, returning matches with their actions. Pure evaluation — no side effects. Defined in `crates/springtale-core/src/rule/`.

**rmcp** — The Rust SDK for Model Context Protocol. Springtale pins `rmcp` 1.x and uses its stdio transport for the MCP bridge.

**rustls** — A TLS implementation written in pure Rust. Springtale uses rustls exclusively — `native-tls` and OpenSSL are banned at compile time via `deny.toml` and a vendor stub at `vendor/native-tls-stub/`.

## S

**Sandbox (WASM)** — The Wasmtime isolation boundary for community connectors. Limits: 10M instruction fuel, 64MB memory (1024 pages), 30-second wall-clock timeout. Only declared capabilities are exposed via the host API. Each invocation gets a fresh `Store` — no cross-call state leakage.

**Sentinel** — The behavioural monitor in `crates/springtale-sentinel/`. Detects toxic capability pairs at manifest install time and writes audit entries to the `audit_trail` table. Always on — there is no "disable sentinel" mode.

**Secret\<T\>** — A wrapper type from the `secrecy` crate. Values inside cannot be logged, cloned, or accidentally serialized. Memory is zeroed on drop via `zeroize`. All credentials in Springtale are `Secret<String>`.

**SMPL** — A Veilid DHT record type that supports multiple writers, each assigned a subkey. Used in Rekindle for governance CRDTs and channel message storage. Maximum 255 writer slots.

**Stage** — A unit of processing in a pipeline. Implements the `Stage` trait with `name()` and `async call(ctx)`. Stages are composed via `compose_pipeline()`. See **Pipeline**.

## T

**Tauri** — A framework for building desktop and mobile apps with web frontends and Rust backends. Springtale's desktop shell uses Tauri 2 with a SolidJS + Tailwind 4 frontend. The desktop app and the web dashboard share a common component library (`tauri/packages/ui`) with a `DataProvider` abstraction: the desktop wraps Tauri `invoke()`, the web wraps HTTP + SSE.

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
