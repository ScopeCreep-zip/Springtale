# Extension points

How to add new functionality to Springtale beyond just writing a
connector. Each section describes a trait you implement, where it
plugs in, and what to do at each step.

These are for **library extension**, not user automation. If you
want to wire a connector together with rules, see
[`docs/guide/rules.md`](../guide/rules.md). If you want to write a
connector, see [`adding-a-connector.md`](adding-a-connector.md).

| Add a… | Trait | Crate |
|---|---|---|
| Transport (between daemons) | `Transport` | `springtale-transport` |
| AI adapter (Claude / Ollama / new vendor) | `AiAdapter` | `springtale-ai` |
| Sentinel check (custom safety verdict) | `SentinelCheck` | `springtale-sentinel` |
| Tick step (custom per-formation work) | `TickStep` | `springtale-bot` |
| Storage backend (Postgres, etc.) | `StorageBackend` | `springtale-store` |
| Approval gate (custom human-in-the-loop) | `ApprovalGate` | `springtale-sentinel` |
| Cooperation primitive | (varies) | `springtale-cooperation` |
| Connector capability | `Capability` | `springtale-connector` |

## Adding a Transport

`Transport` is how daemons talk to each other. Implementations:
`LocalTransport` (Unix socket), `HttpTransport` (rustls mTLS),
`VeilidTransport` (stub for Phase 3).

```rust
// crates/springtale-transport/src/transport/trait_.rs
#[async_trait::async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn send(&self, peer: &PeerId, payload: Bytes) -> Result<(), TransportError>;
    async fn recv(&self) -> Result<(PeerId, Bytes), TransportError>;
    async fn known_peers(&self) -> Vec<PeerId>;
    async fn shutdown(&self) -> Result<(), TransportError>;
}
```

To add one:

1. Create a module under `crates/springtale-transport/src/<your_name>/`.
2. Implement `Transport`. The send/recv are `async`; use `tokio`
   primitives.
3. Register in `crates/springtale-transport/src/lib.rs` and the
   transport factory.
4. Wire config: add a `[transport.<your_name>]` schema variant in
   `crates/springtale-core/src/config/transport.rs`.
5. Tests: integration tests under
   `crates/springtale-transport/tests/`.

Security checklist:

- [ ] All wire data is AEAD-encrypted before send.
- [ ] No plaintext fallback paths.
- [ ] Connection establishment is mutually authenticated.
- [ ] No identifying metadata in plaintext (handshake doesn't leak
      peer identity to passive observers).

See `crates/springtale-transport/src/veilid/stub.rs` for the API
shape against the trait. See `crates/springtale-transport/src/http/`
for a complete implementation.

## Adding an AI adapter

`AiAdapter` is the trait for AI inference. Existing: Anthropic,
Ollama, OpenAI-compat, Noop.

```rust
// crates/springtale-ai/src/adapter/trait_.rs:343
#[async_trait::async_trait]
pub trait AiAdapter: Send + Sync + 'static {
    async fn complete(&self, req: AiRequest, opts: AiOptions)
        -> Result<String, AiError>;
    async fn complete_with_tools(&self, req: AiRequestWithTools, opts: AiOptions)
        -> Result<AiResponseWithTools, AiError>;
    async fn stream(&self, req: AiRequest, opts: AiOptions)
        -> Result<AiStream, AiError>;
    async fn parse_rule(&self, prompt: &str, available_connectors: &[ConnectorInfo])
        -> Result<Rule, AiError>;
    async fn is_available(&self) -> bool;
    // Optional (default: None) — return Some when your vendor supports
    // schema-constrained JSON output. Powers Action::Extract's LlmSchema mode.
    fn structured_extractor(&self) -> Option<&dyn StructuredExtractor>;
}
```

To add one:

1. Create `crates/springtale-ai/src/<your_vendor>/`.
2. Implement `AiAdapter`. Reference `crates/springtale-ai/src/anthropic/adapter.rs`
   or `openai/adapter.rs` for the patterns (HTTP client, SSE
   stream parsing, tool calling).
3. Stream support is mandatory — even if your vendor doesn't
   support streaming natively, fake it by buffering and chunking.
4. The two-layer sanitiser runs **before** your adapter sees the
   prompt. Don't re-implement it.
5. Wire config: `[ai_<your_vendor>]` block. Use `Secret<String>`
   for credentials.
6. Tests: mock the upstream API. See `anthropic::tests` for the
   pattern.

Security checklist:

- [ ] API credentials wrapped in `Secret<String>`.
- [ ] No plaintext credential in URL, env, argv, logs.
- [ ] Stream errors propagate cleanly (caller can distinguish
      "network died" from "model returned error").
- [ ] Tool-call argument JSON is validated before dispatch.
- [ ] Timeout enforcement (don't hang the cooperation tick).

## Adding a sentinel check

Sentinel runs four checks per dispatched action. Adding a fifth
is straightforward.

```rust
// crates/springtale-sentinel/src/check_.rs (conceptual)
#[async_trait::async_trait]
pub trait SentinelCheck: Send + Sync + 'static {
    fn name(&self) -> &str;
    async fn evaluate(&self, ctx: &CheckContext) -> Verdict;
}

pub enum Verdict {
    Go,
    Throttle(Duration),
    Pause { reason: String },
    Quarantine { reason: String },
}
```

The check sees a `CheckContext` with:

- The action being dispatched.
- The connector.
- The current sentinel state (circuit-breaker counters, rate-limit
  windows, etc.).
- The audit-trail history (read-only).

To add:

1. Create your check module under `crates/springtale-sentinel/src/`.
2. Implement `SentinelCheck`.
3. Register in `Sentinel::new()` — push your check into the chain
   at the right order. Order matters: circuit-breaker first
   (cheapest), approval-gate last (most expensive).
4. Audit-trail: every verdict from your check writes to
   `audit_trail` automatically; just emit the right structured
   fields.
5. Tests: deterministic against a synthetic `CheckContext`.

Security checklist:

- [ ] No side effects in `evaluate()` — checks must be pure.
- [ ] Latency target: <500µs p99. If your check needs network or
      disk, refactor it to run async-but-cached.
- [ ] Fail-closed. If your check can't evaluate (e.g. dependency
      unreachable), return `Pause { reason: "..." }`, not `Go`.

## Adding an approval gate

`ApprovalGate` is what the destructive-action sentinel check
delegates to. Default: `DefaultDenyApprovalGate`.

```rust
// crates/springtale-sentinel/src/approval.rs
#[async_trait::async_trait]
pub trait ApprovalGate: Send + Sync + 'static {
    async fn decide(&self, req: ApprovalRequest) -> Decision;
}

pub enum Decision {
    Approved,
    Denied { reason: String },
    Escalated { target: EscalationTarget },
}
```

To add (e.g. for a Slack-based approval gate that DMs an operator):

1. Implement `ApprovalGate` in your crate or in `springtale-sentinel`
   if it's reusable.
2. Wire at sentinel construction:
   ```rust
   Sentinel::with_approval_gate(config, store, Arc::new(MySlackGate::new(...)))
   ```
3. Your `decide` impl should respect a timeout. Default-deny if the
   timeout expires before a decision arrives.
4. Tests: cover the timeout path explicitly.

Security checklist:

- [ ] No bypass on timeout. Timeout → Denied.
- [ ] Audit every decision (approved + denied + escalated).
- [ ] The escalation channel itself must be at least as trusted as
      the action being gated. A "DM an operator over Telegram"
      gate only works if the Telegram bot is trustworthy.

## Adding a tick step

The 14-step formation tick lives in
`crates/springtale-bot/src/runtime/tick_steps/`. Each step is a
module.

To add a step (e.g. a custom step between consensus deadlines and
expire-commits):

1. Create `tick_steps/<your_step>.rs`.
2. Implement the step function with the same signature as
   existing steps:
   ```rust
   pub(crate) async fn run(ctx: &mut TickContext<'_>) -> StepResult { ... }
   ```
3. Register in `tick_steps/mod.rs` and call from
   `event_loop::handle_cadence_tick` at the right position.
4. Bump documented step count in
   [`docs/guide/cooperation.md`](../guide/cooperation.md) §12 and
   [`docs/guide/architecture.md §6`](../guide/architecture.md).

Steps run sequentially. Each step has read-write access to the
formation's state via `TickContext`. Steps should be fast (<5ms);
heavy work goes through the orchestrator (Step 14).

Security checklist:

- [ ] No external network from a tick step. Steps are CPU-only;
      I/O happens via dedicated channels.
- [ ] Errors in your step shouldn't crash the formation. Return
      `StepResult::Error(...)`; the tick continues.

## Adding a storage backend

`StorageBackend` is the trait `springtale-store` exposes. Default:
SQLite via SQLite3MultipleCiphers. In-memory: `MemoryBackend` for
ephemeral mode.

```rust
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    async fn get_rule(&self, id: RuleId) -> Result<Option<Rule>, StoreError>;
    async fn put_rule(&self, rule: Rule) -> Result<(), StoreError>;
    async fn list_rules(&self) -> Result<Vec<Rule>, StoreError>;
    // ... many more methods, one per domain ...
}
```

To add (e.g. Postgres):

1. Create `crates/springtale-store/src/backend/<your_backend>/`.
2. Implement every method on `StorageBackend`. There are ~80;
   start with the SQLite implementation as the reference.
3. Schema: your backend needs to apply the equivalent of
   `schema/sql/*.sql`. SQLite-specific syntax (`AUTOINCREMENT`,
   `TEXT`) will need adapting.
4. Encryption: SQLite handles it via SQLite3MultipleCiphers. For
   Postgres, you'd need to layer AEAD-at-the-row level.
5. Tests: every existing SQLite test should pass against your
   backend too. They're parameterised on the trait.

Security checklist:

- [ ] All data encrypted at rest, AEAD with authenticated
      ciphertexts.
- [ ] Secrets (`Secret<T>`) survive the round-trip — i.e., your
      backend doesn't accidentally serialize them past their
      `Zeroize` boundary.
- [ ] File / connection permissions: equivalent of `0o600` on the
      vault file.
- [ ] No SQL injection — even with a Rust-typed query builder,
      verify every dynamic value is parameter-bound.

We don't currently merge new backends. If Postgres is critical to
your deployment, open a discussion first; we'd want to scope the
review carefully.

## Adding a cooperation primitive

This is the hardest extension point because cooperation primitives
need to compose with the existing 14-step tick.

If you have a new pattern — say, "deferred consensus" or "cross-
formation attention sharing" — the right path is:

1. Open a Discussion with the design.
2. We talk it through against the cooperation framework's
   [design spec](../intended-arch/COOPERATION.md).
3. If accepted, you write a module in
   `crates/springtale-cooperation/src/<your_primitive>/`.
4. Plumb it into the relevant tick step(s) in
   `crates/springtale-bot/src/runtime/tick_steps/`.
5. Write benchmarks
   (`crates/springtale-cooperation/benches/<your_primitive>.rs`).
6. Write a user-facing guide at `docs/guide/<your_primitive>.md`.

The cooperation framework is the most opinionated part of the
codebase. We're glad to extend it but conservative about which
primitives ship.

## Adding a capability

Capabilities are declared in connector manifests and enforced at
dispatch. Existing: `NetworkOutbound`, `FilesystemRead`,
`FilesystemWrite`, `KeychainRead`, `ShellExec`, `BrowserNavigate`.

To add a new capability variant:

1. Add the variant to `Capability` enum in
   `crates/springtale-connector/src/manifest/types.rs`.
2. Implement the runtime check in
   `crates/springtale-connector/src/capability/grant.rs`.
3. Update the toxic-pair detection in
   `crates/springtale-sentinel/src/toxic_pairs.rs` if your
   capability composes dangerously with another.
4. Document at
   [`docs/guide/connectors.md`](../guide/connectors.md) and the
   per-connector reference pages.

Security checklist:

- [ ] No wildcards in the capability value. Exact-match only.
- [ ] Toxic-pair coverage. Combine your new capability with each
      existing one and ask: is this dangerous? If yes, add a
      toxic-pair entry.
- [ ] Capability check is at **dispatch time**, not install time.
      An attacker who modifies a manifest post-install shouldn't be
      able to invoke a capability that wasn't granted.

## Common pitfalls

- **Don't add `unsafe` blocks** without a `// SAFETY: …` comment
  AND a code review with security focus. Most extension points
  are in `forbid(unsafe_code)` crates anyway.
- **Don't introduce new dependencies** without thinking about
  supply chain. `cargo deny check` runs in CI; if your dep brings
  `native-tls`, the build breaks. Good — fix the dep, don't try to
  allowlist it.
- **Don't bypass the sentinel.** If you find yourself wanting to
  dispatch an action without sentinel evaluation, you're in the
  wrong layer. Sentinel is the security boundary; extension code
  goes through it, not around.
- **Don't add new outbound network paths.** The "no telemetry"
  promise is enforced by the absence of code that talks to non-
  configured hosts. Adding outbound is a major-version concern.

## Where to read existing implementations

- Transport: `crates/springtale-transport/src/{local,http,veilid}/`
- AI: `crates/springtale-ai/src/{noop,ollama,anthropic,openai}/`
- Sentinel: `crates/springtale-sentinel/src/{circuit_breaker,rate_limiter,dead_man,approval}.rs`
- Tick steps: `crates/springtale-bot/src/runtime/tick_steps/*.rs`
- Storage: `crates/springtale-store/src/backend/{sqlite,memory}/`
- Capabilities: `crates/springtale-connector/src/capability/`
- Cooperation: `crates/springtale-cooperation/src/*/`

Each is small, focused, and the trait it implements is short. Read
the existing implementations before writing your own.
