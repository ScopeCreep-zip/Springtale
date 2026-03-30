# Phase 1a — Framework + Connectors

> Source: `docs/current-arch/ARCHITECTURE.md` §3, §4, §5, §6.1-6.8
> Depends on: nothing — this is the foundation

## Goal

Ship a typed, signed, sandboxed connector framework with MCP compatibility
and a first-party connector library. Single binary + CLI. No AI required.
Deterministic automation engine.

## What Ships

- `springtaled` — headless daemon (single binary)
- `springtale-cli` — local CLI runner
- Docker Compose config
- 7 first-party connectors (Kick, Presearch, Bluesky, GitHub, filesystem, shell, HTTP)

## Milestone 1: Workspace Scaffolding

Set up the Cargo workspace so everything compiles clean before writing logic.

**How to build:**
- Root `Cargo.toml` with `[workspace]` members and `[workspace.dependencies]` from §5
- Every dependency pinned at workspace root with bounded ranges (`"42"` not `">=42.0.0"`)
- Edition 2024 (stable since Rust 1.85.0)
- Each crate gets a `Cargo.toml` using `workspace = true` for shared deps and a `src/lib.rs` with lint attributes
- App crates get `src/main.rs` with a placeholder `fn main() {}`
- Connectors get `src/lib.rs` stubs

**Crate lint headers (every library crate):**
```rust
#![forbid(unsafe_code)]  // except springtale-crypto and springtale-connector: #![deny(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
```

**Verification:** `cargo build --workspace` and `cargo clippy --workspace` both pass on the empty workspace.

**Integration notes:**
- The `.cargo/config.toml` already exists with rustflags and the native-tls patch
- The `vendor/native-tls-stub/` already exists with `compile_error!`
- The `deny.toml` already exists — run `cargo deny check` to verify no banned deps sneak in
- The `rust-toolchain.toml` pins stable channel with rustfmt + clippy

## Milestone 2: springtale-core

The heart of Springtale. Pipeline composition + rule evaluation. Zero network,
zero crypto, zero AI dependencies. Independently testable.

**How to build:**

The core crate has no dependency on any other springtale crate. It defines the
types and logic that everything else consumes.

**Pipeline system:**
- `PipelineContext<I,O>` carries trace ID, input, output, error list, retry count, and `Attachment` for multimedia
- `Stage` trait: `async fn call(ctx: PipelineContext) -> Result<PipelineContext>` — each stage transforms the context
- `compose_pipeline()` chains stages left-to-right. Failure at any stage short-circuits with `PipelineError` containing the stage index + source error
- Retry graph: configurable per-stage retry with exponential backoff (delegated to springtale-scheduler at runtime)

**Rule system:**
- `Rule` struct: name, description, enabled flag, trigger, conditions, actions. Serializes to/from TOML.
- `Trigger` enum: `Cron`, `FileWatch`, `Webhook`, `ConnectorEvent`, `SystemEvent`. Each carries typed fields (e.g., Cron has expression, FileWatch has path + event type).
- `Condition` enum: tree-structured. `And(Vec<Condition>)`, `Or(Vec<Condition>)`, `Not(Box<Condition>)`, `FieldEquals { field, value }`, `Contains`, `Regex`, `TimeInRange`, `DayOfWeek`. Max depth: 8 (enforced at parse time).
- `Action` enum: `RunConnector { connector, action, params }`, `SendMessage`, `WriteFile`, `RunShell`, `Notify`, `Chain { steps }`, `Transform`, `Delay`. Chain max depth: 4.
- `RuleEngine` loads rules, receives trigger events, evaluates conditions, dispatches action pipelines
- `ConditionEvaluator` is a pure function: `(Condition, serde_json::Value) -> bool`. No side effects.
- Template variables (`${trigger.field}`) resolved in `transform::format`. Sanitized: no nested `${}`, no code execution, string values only.

**Important design detail:** `Action::RunConnector` stores the connector name as a `String`, not a typed reference. springtale-core has no dependency on springtale-connector. The dispatch from action to actual connector call happens in the application layer (springtaled / springtale-bot).

**Research needed:** TOML parsing with `toml` crate — nested tables for conditions and chain steps. `regex` crate timeout configuration (1-second evaluation limit for Condition::Regex).

## Milestone 3: springtale-crypto

All cryptographic operations. This crate uses `#![deny(unsafe_code)]` (not forbid)
because some crypto operations may need audited unsafe blocks.

**How to build:**

**Ed25519 identity:**
- `ed25519_dalek::SigningKey` wrapped in `Secret<SigningKey>` at all times
- Generate via OS CSPRNG (`rand::rngs::OsRng`)
- Persist to vault (encrypted), load on startup
- `NodeId([u8; 32])` newtype over the public key bytes — Veilid-compatible for Phase 3

**Encrypted vault:**
- XChaCha20-Poly1305 AEAD for payload encryption
- Argon2id for passphrase -> encryption key derivation
- 192-bit nonces from OS CSPRNG (collision probability negligible at 2^-96)
- Vault file: no magic bytes, no headers, `.bin` extension
- File created with `0o600` permissions, checked on load
- Canonical JSON (`canonical_json()` sorts keys deterministically) for signing

**Manifest signatures:**
- `sign(bytes, signing_key) -> Signature` — sign connector manifest bytes
- `verify(bytes, signature, public_key) -> Result<()>` — verify before load
- Canonical JSON serialization ensures deterministic byte representation

**Integration notes:**
- Every `.expose_secret()` call site annotated with `// SECURITY: expose needed for X`
- No `derive(Debug)` on any type containing key material
- `zeroize` on drop for all sensitive values via the `zeroize` derive macro

**Research needed:** `ed25519-dalek` 2.x API for signing/verification. `chacha20poly1305` crate AEAD API. `argon2` crate parameter selection (memory cost, time cost, parallelism — document the chosen defaults).

## Milestone 4: springtale-transport (LocalTransport)

Transport abstraction. Phase 1a implements only `LocalTransport` (Unix sockets).

**How to build:**

```rust
pub trait Transport: Send + Sync + 'static {
    async fn send(&self, to: &NodeId, msg: Message) -> Result<(), TransportError>;
    async fn recv(&self) -> Result<(NodeId, Message), TransportError>;
    async fn node_id(&self) -> NodeId;
    fn name(&self) -> &'static str;
}
```

- `TransportError` is a `thiserror` enum (not `anyhow` — this is a library trait)
- `recv()` must be cancel-safe for use in `tokio::select!`
- `Message` contains only `id: Uuid` and `payload: Vec<u8>` — no sender info, no timestamps (handled at payload layer)
- `LocalTransport` uses `tokio::net::UnixListener` + `UnixStream`
- Socket file at configurable path, default `~/.local/share/springtale/springtale.sock`
- Socket created with `0o600` permissions
- Message size limit: 16 MiB (reject larger messages)
- `VeilidTransport` stub: struct with `todo!()` method bodies, gated behind `#[cfg(feature = "veilid")]`

**Integration notes:**
- All application code takes `Arc<dyn Transport + Send + Sync>`
- `runtime::boot` in springtaled instantiates the concrete transport based on config
- Phase 2 adds `HttpTransport`, Phase 3 adds `VeilidTransport` — zero changes to business logic

**Research needed:** tokio Unix socket API. Cancel-safety requirements for `tokio::select!`. Framing protocol over Unix sockets (length-prefixed messages).

## Milestone 5: springtale-connector

Connector runtime. Two trust levels: NativeConnector (first-party Rust, in-process) and WasmConnector (community, sandboxed).

**How to build:**

**Connector trait:**
```rust
pub trait Connector: Send + Sync + 'static {
    fn triggers(&self) -> &[TriggerDecl];
    fn actions(&self) -> &[ActionDecl];
    async fn execute(&self, action: &str, input: serde_json::Value) -> Result<ActionResult>;
    async fn on_event(&self, trigger: &str, handler: EventHandler) -> Result<()>;
    fn manifest(&self) -> &ConnectorManifest;
}
```

**Manifest types:**
- `ConnectorManifest`: name, version, author, description, capabilities, triggers, actions, data_disclosure, wasm_hash (optional), signature
- `Capability` enum: `NetworkOutbound { host }`, `FilesystemRead { path }`, `FilesystemWrite { path }`, `KeychainRead { key }`, `ShellExec`
- No wildcards in NetworkOutbound — exact host match only
- `DataDisclosure`: what user data the connector collects (transparency requirement)

**Native connector loading:**
1. Parse manifest TOML -> `ConnectorManifest` (garde validation)
2. Verify Ed25519 signature
3. Check capabilities against user policy (ShellExec -> prompt approval)
4. Register capability checker that runs before every `execute()`

**WASM connector loading:**
1. Same manifest verification as native
2. Verify WASM binary hash matches `manifest.wasm_hash`
3. Create Wasmtime `Engine` (shared, compiled once) + per-connector `Store`
4. Apply `SandboxLimits`: 10M fuel, 1024 memory pages (64MB), 30s timeout
5. WASI host functions gated by declared capabilities

**Capability enforcement:** `check_capability()` runs BEFORE every `execute()` call for both native and WASM connectors. The check is in the dispatch layer, not in the connector code — connector cannot skip it.

**Research needed:** Wasmtime 43 API — `Engine::new()`, `Store::new()`, fuel metering via `Store::set_fuel()`, memory limits via `StoreLimitsBuilder`. WASI Preview 2 (feature `p2` on wasmtime-wasi). The `host_api.rs` must expose only capability-gated operations to the WASM guest.

## Milestone 6: springtale-scheduler

Cron, filesystem watching, job queue, retry logic.

**How to build:**

- `cron::executor` — parse cron expressions with the `cron` crate, schedule `tokio::time::sleep_until` for next fire time, emit trigger event to router
- `watcher::fs_watcher` — `notify` crate (v8) with debounced events (configurable debounce interval, default 500ms), emit `Trigger::FileWatch` events
- `queue::producer/consumer` — job queue backed by `StorageBackend`. Producer serializes job as JSON, inserts row. Consumer polls with `dequeue_job()` (SQLite `UPDATE ... RETURNING` with row locking), concurrency limit configurable.
- `retry::backoff` — exponential backoff: `base_delay * 2^attempt` with ±10% random jitter. Max attempts configurable per rule.

**Integration notes:**
- Scheduler is initialized in `springtaled` startup and runs alongside the rule engine
- Cron triggers and filesystem triggers feed into the same `router::dispatch` as connector events
- Job queue replaces Redis — SQLite table polled by tokio tasks

**Research needed:** `cron` crate 0.13 API for expression parsing and next-fire calculation. `notify` crate v8 API changes from v7 (feature flag names, event types). SQLite row-locking semantics for concurrent job consumers.

## Milestone 7: springtale-store (SQLite)

All persistence. Phase 1a uses SQLite exclusively.

**How to build:**

`StorageBackend` trait defines all persistence operations. `SqliteBackend` implements it with `rusqlite` (bundled SQLite, WAL mode).

**Tables (001_init.sql):**
- `connectors` — registered connector manifests, enabled/disabled status
- `rules` — rule definitions (TOML serialized), enabled/disabled, trigger type index
- `events` — event log: trigger type, connector name, timestamp, action taken (NOT payload content)
- `jobs` — job queue: payload JSON, status (pending/running/complete/failed), created_at, started_at, attempts

**Important:** No raw SQL strings outside this crate. All other crates access persistence through the `StorageBackend` trait. `rusqlite` parameterized queries only.

**Integration notes:**
- Database file at `~/.local/share/springtale/springtale.db`
- WAL mode enabled on connection (`PRAGMA journal_mode=WAL`)
- File created with `0o600` permissions
- Migration runner embedded — runs on startup before any queries

**Research needed:** `rusqlite` bundled vs system SQLite tradeoffs. WAL mode concurrent reader/writer behavior. Migration embedding pattern (include_str! for SQL files).

## Milestone 8: springtale-ai (NoopAdapter only)

The AI socket. Phase 1a ships only the trait definition and NoopAdapter.

**How to build:**

```rust
pub trait AiAdapter: Send + Sync + 'static {
    async fn complete(&self, request: AiRequest, options: AiOptions) -> Result<AiResponse>;
    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream>;
    async fn parse_rule(&self, intent: &str, connectors: &[ConnectorInfo]) -> Result<Rule>;
    async fn is_available(&self) -> bool;
}
```

- `NoopAdapter` returns `Err(AiError::Disabled)` for all methods, `false` for `is_available()`
- `AiRequest` is a closed enum — type system prevents `Secret<T>` from serializing into it
- `AiOptions { max_tokens: u32, timeout: Duration }` with defaults
- springtale-ai depends on springtale-core (for the `Rule` type in `parse_rule` return)

**Integration notes:**
- springtale-core's pipeline engine handles `AiComplete` action stages by calling the configured adapter
- If `NoopAdapter`, `AiComplete` stages pass through `prev.output` unchanged — pipeline continues
- This "skip on disabled" behavior is in the application dispatch layer, not in core

## Milestone 9: springtale-mcp

Adapt any `Connector` into an MCP server automatically.

**How to build:**

- `server::builder` — takes a `&dyn Connector`, reads its `actions()`, generates MCP tool definitions with JSON Schema (via `schemars` derive on action input types)
- `adapter::connector` — translates MCP `tool/call` into `connector.execute(action, input)`
- `transport::stdio` — wraps `rmcp::StdioServerTransport` for CLI usage (`springtale-cli mcp serve`)

**Integration notes:**
- `rmcp` crate version 0.16 (official Rust MCP SDK). Requires edition 2024.
- The MCP layer inherits the connector sandbox security — tool calls go through the same `check_capability()` dispatch
- Any connector automatically becomes available as an MCP server without connector-side changes
- Schema generation: `#[derive(JsonSchema)]` on action input types, schemars generates JSON Schema

**Research needed:** rmcp 1.x API — `#[tool_router]`, `#[tool]`, `#[tool_handler]` macros, `ServerHandler` trait, stdio transport. schemars v1 derive API for JSON Schema generation.

## Milestone 10: First-Party Connectors (7)

Each follows the structure in `.claude/rules/connector-guidelines.md`.

**connector-kick:** (researched against KickEngineering/KickDevDocs)
- OAuth 2.1 PKCE auth flow via `id.kick.com` (authorize + token endpoints)
- Events via webhooks (not WebSocket — research corrected original assumption)
- Triggers: `chat_message`, `stream_live`, `stream_offline`
- Actions: `send_chat`, `get_channel`, `get_stream`
- Capability: `NetworkOutbound { host: "api.kick.com" }`, `NetworkOutbound { host: "id.kick.com" }`
- Scopes: `user:read`, `channel:read`, `channel:write`, `chat:write`, `events:subscribe`

**connector-presearch:** (researched against Presearch docs + SearXNG)
- REST client for Presearch search API (API key header auth)
- TTL result cache (configurable, default 5 min)
- Triggers: none (search is action-only)
- Actions: `search`, `scrape`
- Note: Presearch API docs are sparse. Phase 1a implements basic search + scrape. Multi-language and safe search deferred until API stabilizes.
- Capability: `NetworkOutbound { host: "presearch.com" }`

**connector-bluesky:** (researched against ATProto docs + Jetstream)
- ATProto session management (`createSession` + `refreshSession`)
- Jetstream URL builder and collection filter types for real-time events
- Jetstream WebSocket subscriber deferred to M11 (requires `tokio-tungstenite` + daemon background task)
- Triggers: `mention`, `follow`, `like`, `repost` (declared; wired when Jetstream subscriber is built)
- Actions: `create_post`, `reply`, `like`, `repost`
- Capability: `NetworkOutbound { host: "bsky.social" }`, `NetworkOutbound { host: "jetstream2.us-west.bsky.network" }`

**connector-github:** (researched against GitHub REST API v3 docs)
- REST API v3 client (GraphQL not needed for declared actions)
- Webhook receiver with HMAC-SHA256 signature verification (`X-Hub-Signature-256`, constant-time comparison)
- Triggers: `push`, `pull_request_opened`, `issue_opened`, `issue_comment` (via webhooks)
- Actions: `create_issue`, `post_comment`, `get_diff`
- Capability: `NetworkOutbound { host: "api.github.com" }`

**connector-filesystem:**
- `notify` crate (v8) filesystem watcher via `notify-debouncer-full` (500ms default debounce)
- Path allow-list enforced via canonicalization (prevents symlink traversal)
- No symlink following (`symlink_metadata` used, symlinks skipped in listings)
- Triggers: `file_created`, `file_modified`, `file_deleted`
- Actions: `read_file`, `write_file`, `list_dir`
- Capability: `FilesystemRead`, `FilesystemWrite` per configured path

**connector-shell:**
- Command allow-list in config (no arbitrary command execution)
- Metacharacter injection filter (pipes, semicolons, backticks, subshells)
- `tokio::process::Command` with configurable timeout (default 30s)
- Requires `ShellExec` capability — blocking approval in CLI (Phase 2b: Tauri modal)
- Actions: `exec`
- stdout/stderr captured, exit code returned

**connector-http:**
- Generic HTTP client via `reqwest` (rustls-tls)
- Host allow-list enforced via URL parsing before every request
- Webhook listener delegated to springtaled management API endpoint (M11)
- Actions: `get`, `post`
- Capability: `NetworkOutbound { host }` per target host

## Milestone 11: Applications

**springtaled:**
Startup order (enforced, each step must succeed before next):
1. Load config from `springtale.toml` (figment: TOML + env vars)
2. Initialize springtale-store (SQLite, run migrations)
3. Initialize springtale-crypto vault (prompt for passphrase if needed)
4. Initialize springtale-transport (LocalTransport for Phase 1a)
5. Initialize springtale-scheduler (cron + watcher + job queue)
6. Load and verify all enabled connectors
7. Start scheduler cron + watcher tasks
8. Start axum management API (last — no requests during boot)
9. Signal readiness (`READY\n` to stdout for process supervisors)

Management API: `/health`, `/ready`, `/connectors` (CRUD), `/rules` (CRUD),
`/events` (paginated log), `/webhook/{connector}/{trigger}` (inbound webhooks).
HMAC bearer token auth. `127.0.0.1:8080` default bind. Rate limiting via tower-http.

**springtale-cli:**
clap derive subcommands: `connector install/list/remove/enable/disable`,
`rule add/list/toggle/run`, `events`, `server start`, `init`.
Output: table format default, `--json` flag for machine parsing.
Vault passphrase via `rpassword` (no echo, not in shell history).

## Milestone 12: Integration Testing

- [ ] End-to-end: `springtale init` -> install connector -> add rule -> trigger -> action executes
- [ ] Connector signature verification: valid manifest loads, tampered manifest rejected
- [ ] Capability enforcement: connector denied access to undeclared host
- [ ] WASM sandbox: fuel exhaustion traps, memory limit traps
- [ ] Rule evaluation: condition tree with nested And/Or/Not
- [ ] Cron trigger: schedule fires and dispatches action
- [ ] Filesystem trigger: file creation detected, rule fires
- [ ] Webhook: inbound POST with valid HMAC -> trigger -> action
- [ ] Management API: all CRUD operations, auth required
- [ ] Docker Compose: single `docker compose up` brings up working instance

## Not In Phase 1a

- No AI adapters beyond NoopAdapter
- No HTTP transport (Unix sockets only)
- No Tauri / desktop UI
- No chat connectors (Telegram is Phase 1b)
- No sentinel behavioral monitor (Phase 2a)
- No bot runtime / command routing (Phase 1b)
- No heartbeat module (Phase 2a)
- No duress/panic features (Phase 2b)
