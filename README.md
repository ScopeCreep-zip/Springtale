<p align="center">
  <img src="docs/logo/Bug.png" alt="Springtale mascot" width="200" />
</p>

**A local-first, privacy-preserving automation platform — no telemetry, no accounts, no central server.**

Springtale is a connector framework that lets you automate across services (chat platforms, APIs, filesystems, search engines) without trusting any single connector with your data, your credentials, or your identity. Designed for people whose safety depends on privacy.

Everything works without AI. Plug in a local model or API key if you want. Unplug it and nothing breaks.

## Why Springtale?

Existing automation and agent platforms share the same problems: they run
untrusted code with full machine access, store secrets in plaintext, phone home
to central servers, and require phone numbers or emails that link your identity
across contexts. If you're someone who faces real consequences when your identity
leaks, those aren't acceptable tradeoffs.

- **No telemetry** — not opt-out, not anonymized. It doesn't exist.
- **No phone number** — no email, no real name. Identity is an Ed25519 keypair
  generated locally on your machine.
- **No central server** — self-hosted now. Planned P2P over Veilid.
- **No trust required** — community connectors run in a WASM sandbox with signed
  manifests and declared capabilities. They cannot access anything you don't approve.
- **No AI dependency** — the default adapter is `NoopAdapter`. Rules, automations,
  and bot commands work without any AI plugged in.

## Quick Start

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
cargo build --workspace
cargo run --bin springtale-cli -- init          # creates encrypted vault + database
cargo run --bin springtale-cli -- server start  # starts daemon on 127.0.0.1:8080
```

With Nix: `direnv allow` loads the full dev shell via Konductor. With Docker: `docker compose up -d`.

Full walkthrough with a worked example: [docs/QUICKSTART.md](docs/QUICKSTART.md)

## Connectors

Fourteen first-party connectors ship today. You wire them together with rules — no code, no AI, just TOML:

| Connector | Platform | What it does |
|---|---|---|
| `connector-kick` | Kick streaming | OAuth 2.1 PKCE, chat, stream events, webhooks |
| `connector-bluesky` | Bluesky / ATProto | posts, replies, likes, Jetstream firehose |
| `connector-github` | GitHub | issues, comments, diffs, HMAC webhooks |
| `connector-presearch` | Presearch | privacy-first search + scraping, cached |
| `connector-filesystem` | Local files | watch, read, write with path allow-lists |
| `connector-shell` | Shell commands | execute with allow-list, timeout, approval gate |
| `connector-http` | Generic HTTP | GET/POST/PUT/DELETE with host allow-list |
| `connector-telegram` | Telegram | Bot API, polling + webhooks |
| `connector-discord` | Discord | twilight gateway, slash commands, messages |
| `connector-slack` | Slack | Socket Mode + webhooks, messages, blocks |
| `connector-irc` | IRC | native IRC client, channels, messages |
| `connector-nostr` | Nostr | NIP-44 relays, notes, encrypted DMs |
| `connector-signal` | Signal | signal-cli bridge, messages, groups |
| `connector-browser` | Headless browser | Chromium via WASM, navigate, click, screenshot |

`connector-matrix` is deferred — `matrix-sdk` pins a `rusqlite` with an open heap-leak CVE; Springtale uses the patched version. We'll ship it once upstream catches up.

Any connector automatically becomes an MCP server via `springtale-mcp`. One framework, not N hand-written servers.

### Example: Kick Stream → Bluesky Post

```toml
[rule]
name = "stream-announce"

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${trigger.broadcaster.username} is live: ${trigger.title}"
```

Per-connector details: [docs/reference/connectors/](docs/reference/connectors/) | How connectors work: [docs/guide/connectors.md](docs/guide/connectors.md)

## Architecture

Springtale is a Rust workspace with strict downward-only dependencies. No cycles, no upward references.

```
┌──────────────────────────────────────────────────────────┐
│                      Applications                       │
│   springtaled (daemon)         springtale-cli            │
│   Tauri desktop + web dashboard (SolidJS)                │
├──────────────────────────────────────────────────────────┤
│                        Bot Layer                         │
│   bot (runtime, router, cooperation, orchestrator)       │
│   runtime (shared init, dispatch, operations)            │
├──────────────────────────────────────────────────────────┤
│                   Integration Crates                     │
│   mcp (rmcp 1.x bridge)     ai (Anthropic/Ollama/…/Noop) │
│   sentinel (toxic pairs, behavioural monitor)            │
│   scheduler (cron, watcher, jobs, heartbeat)             │
├──────────────────────────────────────────────────────────┤
│                     Connector Layer                      │
│   trait, registry, manifest signing, capability system,  │
│   WASM sandbox (Wasmtime — fuel, memory, epoch timeout)  │
├──────────────────────────────────────────────────────────┤
│                    Foundation Crates                     │
│   store (SQLite + WAL + 8 migrations)                    │
│   crypto (Ed25519, vault, Argon2id, XChaCha20-Poly1305)  │
│   core (rule engine, pipeline, transforms, canvas)       │
│   transport (Local / HTTP-mTLS / Veilid-stub)            │
└──────────────────────────────────────────────────────────┘
```

*Fig. 1. Crate stack. Dependencies flow downward only.*

When an event arrives — webhook, file change, cron timer, chat message — it flows through trigger matching, condition evaluation, pipeline stages, capability check, then dispatch to the connector. Events and outputs land in the store. Bots sit on top of the same rule engine and add command routing, session memory, and a cooperation framework that coordinates multi-agent formations without central orchestration.

Architecture (as-built): [docs/arch/ARCHITECTURE.md](docs/arch/ARCHITECTURE.md) · [docs/arch/SECURITY.md](docs/arch/SECURITY.md) · [docs/arch/AUDIT-NOTES.md](docs/arch/AUDIT-NOTES.md)
Design intent: [docs/current-arch/ARCHITECTURE.md](docs/current-arch/ARCHITECTURE.md)
Friendly guide: [docs/guide/architecture.md](docs/guide/architecture.md)

## Security

Security and privacy are constraints, not features. Eight independent layers — compromise of one doesn't cascade to the others.

```
┌───────────────────────────────────────────────────────────┐
│  Zero Telemetry — nothing leaves your device              │
├───────────────────────────────────────────────────────────┤
│  Transport Encryption — rustls only, OpenSSL banned       │
├───────────────────────────────────────────────────────────┤
│  Vault Encryption — XChaCha20-Poly1305 + Argon2id KDF     │
├───────────────────────────────────────────────────────────┤
│  WASM Sandbox — fuel metering, memory limits, timeout     │
├───────────────────────────────────────────────────────────┤
│  Capability Model — exact-host matching, toxic pair block │
├───────────────────────────────────────────────────────────┤
│  Manifest Signing — Ed25519, verify on every load         │
├───────────────────────────────────────────────────────────┤
│  Secret<T> — can't log, clone, or serialize; zeroed       │
├───────────────────────────────────────────────────────────┤
│  Supply Chain — cargo-deny, cargo-audit, gitleaks in CI   │
└───────────────────────────────────────────────────────────┘
```

*Fig. 2. Defence-in-depth. Compromise of any one layer doesn't cascade.*

Native connectors (first-party, audited) run in-process with runtime capability checks. Community WASM connectors are fully sandboxed in Wasmtime — they can only reach the host through capabilities declared in their signed manifest.

Security guide: [docs/guide/security.md](docs/guide/security.md) | Full threat model + OWASP/MITRE mappings: [docs/current-arch/SECURITY.md](docs/current-arch/SECURITY.md)

## Roadmap

| Phase | Scope | State |
|---|---|---|
| 1a | Framework + connectors. Daemon, CLI, rule engine, crypto vault, WASM sandbox, 7 baseline connectors, MCP bridge. | Present. |
| 1b | Bot foundations. `springtale-bot` with command router, cooperation framework, `connector-telegram`. | Present. 12 of 20 cooperation modules are type-defined only; 3 unaudited. |
| 2a | Chat + AI. Discord, Signal, IRC, Slack, Nostr connectors. Anthropic / Ollama / OpenAI-compat adapters. `HttpTransport` (rustls mTLS). `springtale-sentinel`. | Present except: OpenAI streaming is stubbed (non-streaming works). `connector-matrix` is not in the workspace — `matrix-sdk` pins a `rusqlite` with an open CVE. |
| 2b | Desktop + safety. Tauri 2 shell, SolidJS dashboard, canvas visualisation. Duress vault, panic wipe, travel mode. | Shell, dashboard, canvas, duress, panic wipe, travel mode all present. Visual rule builder, i18n, and a11y are not implemented. |
| 3 | Veilid mesh. P2P transport, E2E encrypted AI chat, no server. | `VeilidTransport` is a stub — every method returns `TransportError::NotConnected`. |

Full breakdown: [docs/ROADMAP.md](docs/ROADMAP.md)

## Ecosystem

Springtale draws from and contributes to a constellation of projects:

- **[Rekindle](https://github.com/ScopeCreep-zip/Rekindle)** — Veilid-native decentralized gaming chat. Springtale's `VeilidTransport` (currently a stub) targets `rekindle-protocol`.
- **[Konductor](https://github.com/braincraftio/konductor)** — Nix flake framework for reproducible dev environments.
- **[Kalilix](https://github.com/ScopeCreep-zip/kalilix)** — Nix-based polyglot dev environment with security tooling.
- **[SpiritStream](https://github.com/ScopeCreep-zip/SpiritStream)** — Tauri desktop app patterns.

## 📚 Documentation

| New to Springtale? | Looking something up? | Want to contribute? |
|---|---|---|
| [Learning path](docs/guide/) | [CLI reference](docs/reference/cli.md) | [Contributing guide](docs/contributing/) |
| [Architecture guide](docs/guide/architecture.md) | [API reference](docs/reference/api.md) | [Design decisions](docs/contributing/design-decisions.md) |
| [Security guide](docs/guide/security.md) | [Config reference](docs/reference/configuration.md) | [Adding a connector](docs/contributing/adding-a-connector.md) |
| [Glossary](docs/GLOSSARY.md) | [Connector reference](docs/reference/connectors/) | [As-built arch](docs/arch/) |

## Contributing

Springtale is built by and for marginalized communities.
We welcome security review, accessibility expertise, i18n, and code.
Start with [docs/contributing/](docs/contributing/).

## License

[MIT](LICENSE) — ScopeCreep.zip, 2026
