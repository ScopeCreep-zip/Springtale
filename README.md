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

Seven first-party connectors ship with Phase 1a. You wire them together with rules — no code, no AI, just TOML:

| Connector | Platform | Triggers | Actions |
|-----------|----------|----------|---------|
| `connector-kick` | Kick streaming | 4 (chat, stream live/offline, follow) | 3 (send chat, get channel/stream) |
| `connector-bluesky` | Bluesky/ATProto | 4 (mention, follow, like, repost) | 4 (post, reply, like, repost) |
| `connector-github` | GitHub | 4 (push, PR, issue, comment) | 3 (create issue, comment, get diff) |
| `connector-presearch` | Presearch | — | 2 (search, scrape with caching) |
| `connector-filesystem` | Local files | 3 (create, modify, delete) | 3 (read, write, list) |
| `connector-shell` | Commands | — | 1 (exec with allow-list) |
| `connector-http` | Generic HTTP | — | 2 (get, post with host allow-list) |

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
┌─────────────────────────────────────────────────────────┐
│                     Applications                        │
│  springtaled (daemon)        springtale-cli (terminal)  │
├─────────────────────────────────────────────────────────┤
│                   Integration Crates                    │
│  mcp (protocol bridge)   ai (adapter + noop)            │
│  scheduler (cron, watcher, jobs, retry)                 │
├─────────────────────────────────────────────────────────┤
│                     Connector Layer                     │
│  trait, registry, manifest signing, capability system,  │
│  WASM sandbox (Wasmtime — fuel, memory, timeout)        │
├─────────────────────────────────────────────────────────┤
│                    Foundation Crates                     │
│  store (SQLite + WAL)           crypto (Ed25519, vault) │
│  core (rule engine, pipeline)   transport (Unix socket) │
└─────────────────────────────────────────────────────────┘
```

When an event arrives — webhook, file change, cron timer — it flows through trigger matching, condition evaluation, pipeline stages, capability check, then dispatch to the connector. Events are logged to the store. The job queue runs 4 concurrent workers.

Architecture guide: [docs/guide/architecture.md](docs/guide/architecture.md) | Full specification: [docs/current-arch/ARCHITECTURE.md](docs/current-arch/ARCHITECTURE.md)

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

Native connectors (first-party, audited) run in-process with runtime capability checks. Community WASM connectors are fully sandboxed in Wasmtime — they can only reach the host through capabilities declared in their signed manifest.

Security guide: [docs/guide/security.md](docs/guide/security.md) | Full threat model + OWASP/MITRE mappings: [docs/current-arch/SECURITY.md](docs/current-arch/SECURITY.md)

## Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 1a | Framework + Connectors | **COMPLETE** |
| 1b | Bot Foundations | IN DESIGN — command routing, Telegram connector |
| 2a | Chat + AI | PLANNED — Discord, Signal, WhatsApp, Matrix, IRC, Slack, Nostr |
| 2b | Desktop + Mobile + Safety | PLANNED — Tauri 2, duress passphrase, panic wipe, travel mode |
| 3 | Veilid Mesh | PLANNED — P2P transport, E2E encrypted AI chat, no server |

Full roadmap: [docs/ROADMAP.md](docs/ROADMAP.md)

## Ecosystem

Springtale draws from and contributes to a constellation of projects:

- **[Rekindle](https://github.com/ScopeCreep-zip/Rekindle)** — Veilid-native decentralized gaming chat. Phase 3 transport wraps `rekindle-protocol`.
- **[Konductor](https://github.com/braincraftio/konductor)** — Nix flake framework for reproducible dev environments.
- **[Kalilix](https://github.com/ScopeCreep-zip/kalilix)** — Nix-based polyglot dev environment with security tooling.
- **[SpiritStream](https://github.com/ScopeCreep-zip/SpiritStream)** — Tauri desktop app patterns.

## 📚 Documentation

| New to Springtale? | Looking something up? | Want to contribute? |
|---|---|---|
| [Learning path](docs/guide/) | [CLI reference](docs/reference/cli.md) | [Contributing guide](docs/contributing/) |
| [Architecture guide](docs/guide/architecture.md) | [API reference](docs/reference/api.md) | [Design decisions](docs/contributing/design-decisions.md) |
| [Security guide](docs/guide/security.md) | [Config reference](docs/reference/configuration.md) | [Adding a connector](docs/contributing/adding-a-connector.md) |
| [Glossary](docs/GLOSSARY.md) | [Connector reference](docs/reference/connectors/) | [Full architecture spec](docs/current-arch/) |

## Contributing

Springtale is built by and for marginalized communities.
We welcome security review, accessibility expertise, i18n, and code.
Start with [docs/contributing/](docs/contributing/).

## License

[MIT](LICENSE) — ScopeCreep.zip, 2026
