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
| `connector-http` | Generic HTTP | GET/POST with host allow-list |
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
│   bot (runtime, router, cooperation glue, orchestrator)  │
│   runtime (shared init, dispatch, operations)            │
├──────────────────────────────────────────────────────────┤
│                   Integration Crates                     │
│   cooperation (40 pub modules — formations, momentum,       │
│                rally, supervision, gossip, mental model) │
│   mcp (rmcp 1.x bridge)     ai (Anthropic/Ollama/…/Noop) │
│   sentinel (toxic pairs, monitor, approval gate)         │
│   scheduler (cron, watcher, jobs, heartbeat)             │
├──────────────────────────────────────────────────────────┤
│                     Connector Layer                      │
│   trait, registry, manifest signing, capability system,  │
│   WASM sandbox (Wasmtime — fuel, memory, epoch timeout)  │
├──────────────────────────────────────────────────────────┤
│                    Foundation Crates                     │
│   store (SQLite + WAL + declarative schema in            │
│          schema/sql/, apply via PRAGMA user_version)     │
│   crypto (Ed25519, vault, Argon2id, XChaCha20-Poly1305)  │
│   core (rule engine, pipeline, transforms, canvas)       │
│   transport (Local / HTTP-mTLS / Veilid-stub)            │
├──────────────────────────────────────────────────────────┤
│             Cross-language bindings (G3)                 │
│   wit (WIT world for WASM Component Model embedding)     │
│   py  (pyo3 Python bindings, abi3-py39)                  │
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
| 1b | Bot foundations. `springtale-bot` with command router, cooperation framework, `connector-telegram`. | Present. Cooperation framework fully extracted into `springtale-cooperation` (40 pub modules, zero internal deps) and wired through a 14-step formation tick. See [`docs/arch/AUDIT-NOTES.md §3`](docs/arch/AUDIT-NOTES.md). |
| 2a | Chat + AI. Discord, Signal, IRC, Slack, Nostr connectors. Anthropic / Ollama / OpenAI-compat adapters (all three stream). `HttpTransport` (rustls mTLS). `springtale-sentinel`. | Present. `connector-matrix` is not in the workspace — `matrix-sdk` pins a `rusqlite` with an open CVE. |
| 2b | Desktop + safety. Tauri 2 shell, SolidJS dashboard, canvas visualisation. Duress vault, panic wipe, travel mode, disguise tray icon (G5f), OS-wide quick-hide shortcut (G5g), destructive-action approval gate (G5b). | Shell, dashboard, canvas, duress, panic wipe, travel mode, disguise, quick-hide, approval gate all present. Visual rule builder (basic overlay shipped), i18n, and a11y still in progress. |
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
| [Learning path](docs/guide/) | [CLI reference](docs/reference/cli.md) | [CONTRIBUTING.md](CONTRIBUTING.md) |
| [Installation](docs/installation/) | [API reference](docs/reference/api.md) | [Adding a connector](docs/contributing/adding-a-connector.md) |
| [QUICKSTART](docs/QUICKSTART.md) | [Config reference](docs/reference/configuration.md) | [Extension points](docs/contributing/extension-points.md) |
| [Tutorials](docs/tutorials/) | [Connector reference](docs/reference/connectors/) | [Architecture Decision Records](docs/adr/) |
| [Cookbook](docs/cookbook/) | [API client examples](docs/reference/api-clients/) | [Design decisions](docs/contributing/design-decisions.md) |
| [Architecture](docs/guide/architecture.md) | [Python bindings](docs/python/) | [As-built arch](docs/arch/) |
| [Cooperation](docs/guide/cooperation.md) | [Recipe format](docs/reference/recipes-format.md) | [Code of Conduct](CODE_OF_CONDUCT.md) |
| [Recipes](docs/guide/recipes.md) | [Performance reference](docs/reference/performance.md) | [Anonymous contribution](docs/anonymous-contribution.md) |
| [Executions + drift](docs/guide/executions-and-drift.md) | [Glossary](docs/GLOSSARY.md) | [Security disclosure](SECURITY.md) |
| [External workspaces](docs/guide/external-workspaces.md) | [Operations](docs/operations/) | [OPSEC](docs/opsec.md) |
| [Security](docs/guide/security.md) | [Glossary](docs/GLOSSARY.md) | [Anonymous contribution](docs/anonymous-contribution.md) |
| [FAQ](docs/FAQ.md) | [Operations](docs/operations/) | [Security disclosure](SECURITY.md) |
| [Threat model FAQ](docs/threat-model-faq.md) | [CHANGELOG](CHANGELOG.md) | [OPSEC](docs/opsec.md) |

## Contributing

Springtale is built by and for marginalized communities.
We welcome security review, accessibility expertise, i18n, and code.

Start with [CONTRIBUTING.md](CONTRIBUTING.md). Read the
[Code of Conduct](CODE_OF_CONDUCT.md). If you can't put a real name
on your commits, see [docs/anonymous-contribution.md](docs/anonymous-contribution.md).

Found a vulnerability? Don't open an issue. See
[SECURITY.md](SECURITY.md) for the disclosure policy.

## License

[MIT](LICENSE) — ScopeCreep.zip, 2026
