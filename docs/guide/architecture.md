# Architecture

Springtale is a Rust workspace of ~28 crates — 11 library crates, 14 first-party connectors, 2 applications, plus a Tauri frontend (excluded from the workspace, built separately). This guide explains how they fit together.

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
                    │  runtime, router, cooperation,      │   │
                    │  orchestrator, memory, handlers     │   │
                    └──────────────────┬──────────────────┘   │
                                       │                      │
                    ┌──────────────────▼──────────────────┐   │
                    │        springtale-runtime           │◄──┘
                    │  shared init, dispatch, operations  │
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
    └────┬───┘        └───┬────┘  └───┬────┘  └─────┬──────┘  │
         │                │           │             │         │
         └────────────────┴─────┬─────┴─────────────┘         │
                                v                              │
                    ┌──────────────────────────┐               │
                    │    springtale-connector  │               │
                    │  trait, registry,        │               │
                    │  manifest signing,       │               │
                    │  capability system,      │               │
                    │  Wasmtime sandbox        │               │
                    └──────┬──────────┬────────┘               │
                           │          │                        │
                           v          v                        │
                    ┌────────┐   ┌─────────┐                   │
                    │ store  │   │ crypto  │                   │
                    │ SQLite │   │ vault,  │                   │
                    │ + 8    │   │ Ed25519,│                   │
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

*Fig. 1. Crate dependency graph. Arrows point from dependent to dependency. `core` and `crypto` have zero internal Springtale dependencies — they're the foundation everything else builds on.*

### 1.1. What Each Crate Does

**TABLE I. LIBRARY CRATES**

| Crate | One-line purpose |
|---|---|
| `springtale-core` | Rule engine, pipeline composition, event routing, data transforms, canvas types |
| `springtale-crypto` | Ed25519 keypairs, XChaCha20-Poly1305 vault, Argon2id KDF, manifest signatures, mlock |
| `springtale-transport` | `Transport` trait + Local (Unix socket), HTTP (rustls mTLS), Veilid (stub) impls |
| `springtale-connector` | `Connector` trait, Wasmtime WASM sandbox, manifest parser, capability system, registry |
| `springtale-store` | SQLite backend with WAL mode, 8 migrations, AEAD-encrypted bot memory |
| `springtale-scheduler` | Cron executor, filesystem watcher, job queue, heartbeat monitor, exponential backoff |
| `springtale-ai` | `AiAdapter` trait + Noop / Ollama / OpenAI-compat / Anthropic adapters + OWASP sanitiser |
| `springtale-mcp` | MCP protocol bridge (`rmcp` 1.x) — wraps any `Connector` as an MCP server automatically |
| `springtale-sentinel` | Behavioural monitor, toxic-pair capability detection, audit trail |
| `springtale-runtime` | Shared init / dispatch / operations layer used by both the daemon and the Tauri desktop app |
| `springtale-bot` | Bot runtime, command router, handler registry, session memory, cooperation framework, orchestrator |

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
  │  │   OpenAiCompatAdapter ── non-streaming; stream() stubbed  │ │
  │  │   (hot-swappable at runtime via POST /config/ai)         │ │
  │  └──────────────────────────────────────────────────────────┘ │
  │                                                               │
  │  ┌──────────────────────────────────────────────────────────┐ │
  │  │ Connector trait                                          │ │
  │  │                                                          │ │
  │  │   NativeConnector ── in-process (14 connectors present)  │ │
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
         │              • open store + run migrations
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
         │              spawn bot event loop
         │
  8. API server ──────> axum::build_router + bind
         │
  9. Ready ───────────> mark ready=true
                        start API + trigger event loop
                        wait on shutdown signal
```

*Fig. 5. Daemon boot sequence. See [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md) §3 for file:line refs.*

Exposes ~60 REST endpoints for connector management, rule CRUD, formation orchestration, canvas updates, event streaming, configuration, and webhook ingestion. See [reference/api.md](../reference/api.md) for the full endpoint catalogue.

### 5.2. springtale-cli (Terminal)

The CLI for local configuration and management:

```
  springtale init                    create vault + database
  springtale server start            start daemon inline
  springtale connector <subcmd>      install/list/enable/disable/remove
  springtale rule <subcmd>           add/list/toggle/run/update/delete
  springtale events --limit 50       query event log
  springtale agent set-autonomy ...  change autonomy level
  springtale vault duress-setup      configure decoy vault
  springtale vault crypto rotate-vault-key
  springtale travel prepare|restore  wipe/restore for device seizure
  springtale memory audit|compact    inspect/trim bot memory
  springtale data export|purge       export or erase user data
  springtale panic                   emergency wipe (< 3 s)
```

Output defaults to formatted tables. Pass `--json` for machine-readable output. See [reference/cli.md](../reference/cli.md) for full details.

---

## 6. Known Gaps

The following areas diverge from the design intent in `docs/current-arch/`:

| Area | State |
|---|---|
| `connector-matrix` | Not in the workspace. `matrix-sdk` pins `rusqlite` 0.37 with an open heap-leak CVE; Springtale uses the patched 0.39. |
| WASM connectors | The Wasmtime host, capability gate, and SDK exist. All 14 first-party connectors are native Rust; no WASM connector rides the sandbox today. |
| Cooperation wiring | Cadence, momentum, formations, environment, and the orchestrator are wired into the bot event loop. Rally, sacrifice, recovery, consensus, commit, interference, transformation, mental model, and dynamic capability are type-defined but not yet invoked from the hot path. |
| Job queue | `JobProducer` is an in-memory mpsc sender. The `jobs` SQLite table and `StorageBackend` method signatures exist, but the persistent-queue backing is not wired. |
| OpenAI streaming | `OpenAiCompatAdapter::stream()` returns `AiError::NotImplemented`. `complete()` works. Anthropic and Ollama stream fully. |
| `VeilidTransport` | Stub. Every method returns `TransportError::NotConnected`. |
| i18n, a11y | English-only. Screen-reader and keyboard-nav work is not yet done. |

Full detail with rationale: [`docs/arch/AUDIT-NOTES.md`](../arch/AUDIT-NOTES.md). Delivery plan: [ROADMAP.md](../ROADMAP.md).

---

## References

- [1] As-built architecture with file:line refs: [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md)
- [2] Design intent + full threat model: [`docs/current-arch/ARCHITECTURE.md`](../current-arch/ARCHITECTURE.md)
- [3] Cooperation framework: [`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md)
- [4] Crate structure guidelines: `.claude/rules/backend/crate-structure.md`
- [5] Rust conventions: `.claude/rules/backend/rust-conventions.md`
