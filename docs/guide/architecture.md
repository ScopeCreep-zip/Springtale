# Architecture

Springtale is a Rust workspace of 17 crates — 8 libraries, 7 connectors, and 2 applications. This guide explains how they fit together.

For the full specification, see [`docs/current-arch/ARCHITECTURE.md`](../current-arch/ARCHITECTURE.md).

## 1. The Workspace

Every crate has a single responsibility. Dependencies flow strictly downward — no cycles, no upward references.

```
                        ┌─────────────────────────────────────────────┐
                        │                Applications                 │
                        │                                             │
                        │  ┌──────────────┐    ┌────────────────┐     │
                        │  │  springtaled │    │ springtale-cli │     │
                        │  │   (daemon)   │    │   (terminal)   │     │
                        │  └──────┬───────┘    └───────┬────────┘     │
                        └─────────┼────────────────────┼──────────────┘
                                  │                    │
          ┌───────────────────────┼────────────────────┼─────────────┐
          │                       v                    v             │
          │   ┌──────────┐   ┌──────────┐   ┌────────────────┐      │
          │   │   mcp    │   │    ai    │   │   scheduler    │      │
          │   │ MCP      │   │ adapter  │   │ cron, watcher, │      │
          │   │ bridge   │   │ + noop   │   │ jobs, retry    │      │
          │   └────┬─────┘   └────┬─────┘   └───────┬────────┘      │
          │        │              │                  │               │
          │        v              v                  v               │
          │   ┌─────────────────────────────────────────────────┐    │
          │   │                 connector                       │    │
          │   │  trait, registry, manifest, capability, wasm    │    │
          │   └────────────┬────────────────────┬───────────────┘    │
          │                │                    │                    │
          │                v                    v                    │
          │   ┌──────────────────┐   ┌──────────────────┐           │
          │   │      store       │   │      crypto      │           │
          │   │  SQLite backend  │   │ Ed25519, vault,  │           │
          │   │  schema, queries │   │ signatures       │           │
          │   └────────┬─────────┘   └──────────────────┘           │
          │            │                                            │
          │            v                                            │
          │   ┌──────────────────┐   ┌──────────────────┐           │
          │   │       core       │   │    transport     │           │
          │   │  rule engine,    │   │ Unix socket      │           │
          │   │  pipeline,       │   │ (← crypto)       │           │
          │   │  router          │   │                  │           │
          │   │  (zero deps)     │   │                  │           │
          │   └──────────────────┘   └──────────────────┘           │
          │                     Library Crates                      │
          └─────────────────────────────────────────────────────────┘
```

*Fig. 1. Crate dependency graph. Arrows point from dependent to dependency. `core` and `crypto` have zero internal dependencies — they're the foundation everything else builds on.*

### 1.1. What Each Crate Does

**TABLE I. LIBRARY CRATES**

| Crate | One-line purpose |
|-------|-----------------|
| `springtale-core` | Rule engine, pipeline composition, event routing, data transforms |
| `springtale-crypto` | Ed25519 keypairs, XChaCha20-Poly1305 vault, Argon2id KDF, manifest signatures |
| `springtale-transport` | `Transport` trait + Unix socket implementation (Phase 1) |
| `springtale-connector` | `Connector` trait, WASM sandbox, manifest parser, capability system, registry |
| `springtale-store` | SQLite backend with WAL mode, schema for rules/events/jobs/connectors |
| `springtale-scheduler` | Cron executor, filesystem watcher, job queue with retry and backoff |
| `springtale-ai` | `AiAdapter` trait + `NoopAdapter` default (returns fixed response, no AI needed) |
| `springtale-mcp` | MCP protocol bridge — wraps any `Connector` as an MCP server automatically |

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
  │  │   LocalTransport ──── Unix socket (Phase 1) ✓ active   │ │
  │  │   HttpTransport  ──── HTTP/TLS   (Phase 2)   planned   │ │
  │  │   VeilidTransport ─── Veilid P2P (Phase 3)   stub      │ │
  │  └─────────────────────────────────────────────────────────┘ │
  │                                                              │
  │  ┌─────────────────────────────────────────────────────────┐ │
  │  │ AiAdapter trait                                         │ │
  │  │                                                         │ │
  │  │   NoopAdapter ──── "no AI configured" (default) ✓ active│ │
  │  │   Ollama      ──── local models     (Phase 2)   planned│ │
  │  │   OpenAI      ──── API-compatible   (Phase 2)   planned│ │
  │  │   Anthropic   ──── Claude API       (Phase 2)   planned│ │
  │  └─────────────────────────────────────────────────────────┘ │
  │                                                              │
  │  ┌─────────────────────────────────────────────────────────┐ │
  │  │ Connector trait                                         │ │
  │  │                                                         │ │
  │  │   NativeConnector ── in-process, high trust  ✓ 7 shipped│ │
  │  │   WasmConnector  ── sandboxed, low trust       ready    │ │
  │  └─────────────────────────────────────────────────────────┘ │
  └──────────────────────────────────────────────────────────────┘
```

*Fig. 4. Pluggable trait boundaries. Swap any implementation without changing business logic. The `NoopAdapter` proves the entire platform works with zero AI.*

---

## 5. The Applications

### 5.1. springtaled (Daemon)

The headless daemon that runs the show. Boots in this order:

```
  1. Load config ─────> springtale.toml + env overrides
         │
  2. Open store ──────> SQLite with WAL + migrations
         │
  3. Init vault ──────> load or create encrypted vault
         │
  4. Bind transport ──> Unix socket
         │
  5. Load rules ──────> populate RuleEngine from store
         │
  6. Start scheduler ─> cron executor + file watcher
         │
  7. Load connectors ─> register in ConnectorRegistry
         │
  8. Start job queue ─> 4 concurrent action workers
         │
  9. Start API ───────> Axum HTTP on 127.0.0.1:8080
         │
  10. Signal ready ───> GET /ready returns 200
```

*Fig. 5. Daemon boot sequence. Each step depends on the previous one completing.*

Exposes 14 REST endpoints for connector management, rule CRUD, event queries, and webhook ingestion. See [reference/api.md](../reference/api.md) for the full endpoint list.

### 5.2. springtale-cli (Terminal)

The CLI for local configuration and management:

```
  springtale init                          create vault + database
  springtale server start                  start daemon inline
  springtale connector install <manifest>  install from TOML manifest
  springtale connector list                list installed connectors
  springtale connector enable <name>       enable a connector
  springtale connector disable <name>      disable a connector
  springtale connector remove <name>       remove a connector
  springtale rule add <file>               add rule from TOML/JSON
  springtale rule list                     list all rules
  springtale rule toggle <id>              toggle enabled/disabled
  springtale rule run <id>                 dry-run evaluation
  springtale events --limit 50             query event log
```

Output defaults to formatted tables. Pass `--json` for machine-readable output. See [reference/cli.md](../reference/cli.md) for full details.

---

## 6. What's Not Built Yet

Phase 1a is complete. These are stubbed or planned for later phases:

| Component | Phase | Current State |
|-----------|-------|--------------|
| `springtale-bot` | 1b | Commented out in `Cargo.toml` |
| `springtale-sentinel` | 2a | Commented out in `Cargo.toml` |
| `connector-telegram` | 1b | Commented out in `Cargo.toml` |
| `VeilidTransport` | 3 | Returns `NotConnected` errors (trait stub) |
| AI adapters (Ollama, OpenAI, etc.) | 2a | Only `NoopAdapter` ships |
| TypeScript connector SDK | 2a | CI checks for it but not in repo |
| Tauri desktop shell | 2b | Separate project |

See [ROADMAP.md](../ROADMAP.md) for the full delivery plan.

---

## References

- [1] Full architecture specification: [`docs/current-arch/ARCHITECTURE.md`](../current-arch/ARCHITECTURE.md)
- [2] Crate structure guidelines: `.claude/rules/crate-structure.md`
- [3] Rust conventions: `.claude/rules/rust-conventions.md`
