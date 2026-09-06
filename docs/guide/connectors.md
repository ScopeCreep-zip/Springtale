# Connectors

Connectors are adapters between Springtale and the outside world. Each connector talks to one service — Kick, GitHub, Bluesky, your local filesystem — and exposes a uniform interface of **triggers** (events the service emits) and **actions** (things you can tell the service to do).

For per-connector details (config fields, triggers, actions, examples), see [reference/connectors/](../reference/connectors/).

## 1. Two Trust Levels

Not all connectors are created equal. Springtale draws a hard line between code it audits and code it doesn't.

```
  ┌─ Native Connectors (in-process) ─────────────────────────────┐
  │                                                               │
  │   connector-kick        connector-bluesky    connector-http   │
  │   connector-github      connector-presearch                   │
  │   connector-filesystem  connector-shell                       │
  │   connector-telegram    connector-discord    connector-slack  │
  │   connector-irc         connector-nostr      connector-signal │
  │   connector-browser                                           │
  │                                                               │
  │   Trust:     HIGH — audited, signed by Springtale team        │
  │   Isolation: capability-checked at runtime                    │
  │   Language:  Rust (compiled into springtaled)                 │
  │   Limits:    no fuel/memory/timeout constraints               │
  └───────────────────────────────────────────────────────────────┘

  ┌─ WASM Connectors (sandboxed) ─────────────────────────────────┐
  │                                                                │
  │   ┌──────────────────────────────────────────────────────┐     │
  │   │               Wasmtime Sandbox                       │     │
  │   │                                                      │     │
  │   │    community-connector.wasm                          │     │
  │   │                                                      │     │
  │   │    Fuel:    10M instructions   Memory:  64 MB        │     │
  │   │    Timeout: 30 seconds         Output:  1 MB         │     │
  │   │                                                      │     │
  │   │    Host API: only declared capabilities exposed      │     │
  │   └──────────────────────────────────────────────────────┘     │
  │                                                                │
  │   Trust:     LOW — community-authored, untrusted               │
  │   Isolation: Wasmtime sandbox with hard resource limits        │
  │   Language:  TypeScript SDK → wasm32-wasip2, or Rust           │
  └────────────────────────────────────────────────────────────────┘
```

*Fig. 1. Connector trust boundary. Native connectors run in the daemon process with runtime capability checks. WASM connectors are isolated with instruction, memory, and time limits.*

---

## 2. The Capability System

Every connector declares what it needs in its manifest. Springtale enforces these at install time and again at every execution.

**TABLE I. CAPABILITY TYPES**

| Capability | Parameter | What it grants |
|-----------|-----------|---------------|
| `NetworkOutbound` | `host: String` | HTTP/S requests to exactly this host. No wildcards. |
| `FilesystemRead` | `path: String` | Read access to this path (canonicalized, no symlink traversal) |
| `FilesystemWrite` | `path: String` | Write access to this path |
| `KeychainRead` | `key: String` | Read a specific secret from the vault |
| `ShellExec` | (none) | Execute shell commands. **Always requires explicit user approval.** |

Why exact-host matching instead of wildcards: a connector that needs `api.github.com` shouldn't be able to silently also talk to `evil.example.com`. Every host is an explicit decision.

### 2.1. Toxic Pairs

Some capability combinations are dangerous even if each is reasonable alone. These are blocked at install time — no override:

- `KeychainRead` + `NetworkOutbound` (different host) — credential exfiltration
- `FilesystemRead` + `NetworkOutbound` (different host) — file exfiltration
- `ShellExec` + `NetworkOutbound` — command execution + data exfiltration
- `BrowserNavigate` + `KeychainRead` — credential theft via browser
- `FilesystemWrite` + `ShellExec` — write-then-execute attack

---

## 3. Installation Lifecycle

```
  Developer                    springtaled
  (or user)                    Runtime
       │                          │
       │  springtale connector    │
       │  install manifest.toml   │
       ├─────────────────────────>│
       │                          │
       │                    ┌─────┴──────────────────────────┐
       │                    │ 1. Parse TOML manifest         │
       │                    │ 2. Verify Ed25519 signature    │
       │                    │    (if signed)                 │
       │                    │ 3. Extract capabilities        │
       │                    │ 4. Check for toxic pairs       │
       │                    │ 5. Validate action/trigger     │
       │                    │    schemas                     │
       │                    └─────┬──────────────────────────┘
       │                          │
       │                          v
       │                    ┌──────────┐
       │                    │ Register │
       │                    │ in store │
       │                    │ + registry│
       │                    └──────────┘
       │                          │
       │  "installed: connector-  │
       │   kick v0.1.0"          │
       │<─────────────────────────┤
```

*Fig. 2. Connector installation flow. Signature verification and toxic pair checks happen before the connector is registered.*

---

## 4. How Dispatch Works

When a rule's action says `RunConnector`, here's what happens:

```
  RuleEngine                  Dispatch                    Connector
       │                         │                           │
       │  RuleMatch with         │                           │
       │  action: RunConnector   │                           │
       ├────────────────────────>│                           │
       │                         │                           │
       │                   ┌─────┴──────────────────┐       │
       │                   │ 1. Look up connector   │       │
       │                   │    in registry          │       │
       │                   │ 2. Check capability for │       │
       │                   │    this specific action │       │
       │                   │ 3. Resolve template     │       │
       │                   │    variables in params  │       │
       │                   └─────┬──────────────────┘       │
       │                         │                           │
       │                         │  connector.execute(       │
       │                         │    action, params)        │
       │                         ├──────────────────────────>│
       │                         │                           │
       │                         │<──── ActionResult ────────┤
       │                         │                           │
       │                   ┌─────┴──────────────────┐       │
       │                   │ 4. Log event to store  │       │
       │                   │ 5. Return result       │       │
       │                   └────────────────────────┘       │
```

*Fig. 3. Action dispatch with capability checking. The capability check happens at dispatch time, not just at install time — a connector can't exceed its declared permissions even if the registry is somehow corrupted.*

---

## 5. First-Party Connectors

Fifteen native connectors ship today. `connector-matrix` is deferred — `matrix-sdk` pins `rusqlite` 0.37 which has CVE-2025-70873 (heap info disclosure); Springtale uses the patched 0.39.

**TABLE II. FIRST-PARTY CONNECTORS**

| Connector | Platform | Auth | Key Capabilities |
|---|---|---|---|
| `connector-kick` | Kick streaming | OAuth 2.1 PKCE | `NetworkOutbound` (api.kick.com, id.kick.com) |
| `connector-presearch` | Presearch search | API key | `NetworkOutbound` (presearch.com + configured hosts) |
| `connector-bluesky` | Bluesky / ATProto | Session auth | `NetworkOutbound` (bsky.social, jetstream) |
| `connector-github` | GitHub | PAT / webhook HMAC | `NetworkOutbound` (api.github.com) |
| `connector-filesystem` | Local filesystem | — | `FilesystemRead` / `FilesystemWrite` (configured paths) |
| `connector-shell` | Shell commands | — | `ShellExec` (requires approval) |
| `connector-http` | Generic HTTP | — | `NetworkOutbound` (configured hosts) |
| `connector-opencode` | OpenCode (agentic coding) | basic auth (optional) | `NetworkOutbound` (loopback `opencode serve` daemon) |
| `connector-telegram` | Telegram Bot API | bot token | `NetworkOutbound` (api.telegram.org) |
| `connector-discord` | Discord | bot token | `NetworkOutbound` (discord.com, gateway.discord.gg) |
| `connector-slack` | Slack | app + bot tokens | `NetworkOutbound` (slack.com) |
| `connector-irc` | IRC | SASL (optional) | `NetworkOutbound` (configured IRC server) |
| `connector-nostr` | Nostr | nsec / Ed25519 | `NetworkOutbound` (configured relays) |
| `connector-signal` | Signal | signal-cli bridge | `ShellExec` (to signal-cli) |
| `connector-browser` | Headless Chromium | — | `NetworkOutbound` (configured hosts) |

For full details on each connector — config fields, trigger payloads, action inputs/outputs, and example rules — see the [reference/connectors/](../reference/connectors/) directory.

---

## 6. MCP Bridge

> **Not wired up.** `springtale-mcp` builds — `ConnectorMcpServer::new(connector,
> capability_checker)` adapts the `Connector` trait to MCP's tool interface, and
> `transport/stdio.rs` serves it on stdin/stdout — but nothing reaches it.
> `springtaled` lists `springtale-mcp` as a dependency and never uses it; there
> is no `/mcp` route. The CLI has no `mcp` subcommand, so the
> `springtale-cli mcp serve` entry point named in `stdio.rs`'s own doc comment
> does not exist. `start_stdio_server` has no caller outside the crate. Read
> this section as the design, not as something you can run today.

The bridge is **per connector**, not registry-wide: one `ConnectorMcpServer`
wraps exactly one `Arc<dyn Connector>`. It adapts the `Connector` trait to
MCP's tool interface, so you would not write a separate MCP server per
connector.

```
  AI Client                springtale-mcp              Connector
  (Claude, etc.)           Bridge                      (any)
       │                        │                         │
       │  MCP tool call         │                         │
       │  (stdio transport)     │                         │
       ├───────────────────────>│                         │
       │                        │                         │
       │                  ┌─────┴────────────────┐       │
       │                  │ 1. Map MCP tool name │       │
       │                  │    to connector action│       │
       │                  │ 2. Validate input via │       │
       │                  │    JSON Schema        │       │
       │                  │ 3. Capability check   │       │
       │                  └─────┬────────────────┘       │
       │                        │  execute()              │
       │                        ├────────────────────────>│
       │                        │                         │
       │                        │<──── result ────────────┤
       │                        │                         │
       │  MCP tool result       │                         │
       │<───────────────────────┤                         │
```

*Fig. 4. MCP bridge, as designed. One `ConnectorMcpServer` per connector, over stdio. Nothing in the daemon or the CLI currently constructs one.*

This means:
- 15 connectors = 15 MCP servers, automatically
- No MCP-specific code in connectors
- Same capability checks apply at both `list_tools` and `call_tool` — MCP doesn't bypass the sandbox
- Input validated against JSON Schema (via the `jsonschema` crate) at runtime
- Uses `rmcp` 1.x over stdio transport

---

## References

- [1] Per-connector reference: [reference/connectors/](../reference/connectors/)
- [2] Connector authoring guide: [contributing/adding-a-connector.md](../contributing/adding-a-connector.md)
- [3] Full connector framework spec: `docs/current-arch/ARCHITECTURE.md` §6.4, §7
- [4] MCP protocol: `https://modelcontextprotocol.io`
