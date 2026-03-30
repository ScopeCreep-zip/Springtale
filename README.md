<p align="center">
  <img src="docs/logo/Bug.png" alt="Springtale mascot" width="200">
</p>

# Springtale

Local-first, privacy-preserving automation platform. Connector infrastructure first, AI consumer second. Built for people whose safety depends on privacy.

---

## Why

Existing automation platforms run untrusted code with full machine access, store secrets in plaintext, phone home to central servers, and require phone numbers or emails that link your identity across contexts. If you've been doxxed, surveilled, deplatformed, or worse — those aren't acceptable tradeoffs.

- **No telemetry.** Not opt-out. Not anonymized. It doesn't exist.
- **No phone number.** No email. No real name. Identity is a keypair.
- **No central server.** Self-hosted now. Fully P2P via Veilid in Phase 3.
- **No trust required.** Community connectors run in a WASM sandbox with signed manifests and declared capabilities.
- **No AI dependency.** Everything works without AI. Plug one in if you want. Unplug it and nothing breaks.

---

## Quick Start

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
cargo build --workspace
cargo run --bin springtale-cli -- init          # creates vault + database
cargo run --bin springtale-cli -- server start  # starts daemon on 127.0.0.1:8080
```

With Nix: `direnv allow` loads all tools. With Docker: `docker compose up -d`.

Full walkthrough with a worked example: [docs/QUICKSTART.md](docs/QUICKSTART.md)

---

## What You Can Do With It

Springtale ships 7 connectors that talk to external services and local resources. You wire them together with rules — no code, no AI, just TOML:

**TABLE I. CONNECTORS**

| Connector | Platform | Triggers | Actions |
|-----------|----------|----------|---------|
| `connector-kick` | Kick streaming | 4 (chat, stream live/offline, follow) | 3 (send chat, get channel/stream) |
| `connector-bluesky` | Bluesky/ATProto | 4 (mention, follow, like, repost) | 4 (post, reply, like, repost) |
| `connector-github` | GitHub | 4 (push, PR, issue, comment) | 3 (create issue, comment, get diff) |
| `connector-presearch` | Presearch | — | 2 (search, scrape) |
| `connector-filesystem` | Local files | 3 (create, modify, delete) | 3 (read, write, list) |
| `connector-shell` | Commands | — | 1 (exec) |
| `connector-http` | Generic HTTP | — | 2 (get, post) |

Per-connector details: [docs/reference/connectors/](docs/reference/connectors/)

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

Any connector also works as an MCP server automatically via `springtale-mcp` — one framework, not N hand-written servers.

Rule authoring guide: [docs/guide/rules.md](docs/guide/rules.md)

---

## How It Works

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
          │   ┌──────────┐   ┌──────────┐    ┌────────────────┐      │
          │   │   mcp    │   │    ai    │    │   scheduler    │      │
          │   └────┬─────┘   └────┬─────┘    └───────┬────────┘      │
          │        │              │                  │               │
          │        v              v                  v               │
          │   ┌─────────────────────────────────────────────────┐    │
          │   │                 connector                       │    │
          │   │  trait, registry, manifest, capability, wasm    │    │
          │   └────────────┬────────────────────┬───────────────┘    │
          │                │                    │                    │
          │                v                    v                    │
          │   ┌──────────────────┐    ┌──────────────────┐           │
          │   │      store       │    │      crypto      │           │
          │   │    (SQLite)      │    │   (Ed25519,      │           │
          │   │                  │    │    vault)        │           │
          │   └────────┬─────────┘    └──────────────────┘           │
          │            │                                             │
          │            v                                             │
          │   ┌──────────────────┐    ┌──────────────────┐           │
          │   │      core        │    │    transport     │           │
          │   │  (rule engine,   │    │  (Unix socket)   │           │
          │   │   pipeline)      │    │                  │           │
          │   └──────────────────┘    └──────────────────┘           │
          │                      Library Crates                      │
          └──────────────────────-───────────────────────────────────┘
```

*Fig. 1. Crate dependency graph. 8 libraries, 7 connectors, 2 apps. ~20K lines of Rust.*

When an event arrives — webhook, file change, cron timer — it flows through:

```
  External Service ──> springtaled ──────────────────────> Connector
                           │                                  │
                     1. match trigger                         │
                     2. evaluate conditions                   │
                     3. run pipeline stages                   │
                     4. capability check ──> 5. execute() ───>│
                                                              │
                     6. log event <────────── result <────────┘
```

*Fig. 2. Event flow. Capability checks happen before every dispatch — a connector can never exceed its declared permissions.*

Architecture guide: [docs/guide/architecture.md](docs/guide/architecture.md) | Full specification: [docs/current-arch/ARCHITECTURE.md](docs/current-arch/ARCHITECTURE.md)

---

## Security

Security and privacy are constraints, not features. Eight independent layers — compromise of one doesn't cascade:

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
  │ Secret<T> — can't log, clone, or serialize; zeroed on drop│
  ├───────────────────────────────────────────────────────────┤
  │  Supply Chain — cargo-deny, cargo-audit, gitleaks in CI   │
  └───────────────────────────────────────────────────────────┘
```

*Fig. 3. Defense-in-depth stack.*

Native connectors (first-party, audited) run in-process with capability checks. Community WASM connectors are fully sandboxed in Wasmtime — they can only reach the host through capabilities declared in their signed manifest.

Security guide: [docs/guide/security.md](docs/guide/security.md) | Full threat model + OWASP/MITRE mappings: [docs/current-arch/SECURITY.md](docs/current-arch/SECURITY.md)

---

## Roadmap

**TABLE II. PHASE STATUS**

| Phase | Name | Status |
|-------|------|--------|
| 1a | Framework + Connectors | **COMPLETE** — daemon, CLI, 8 crates, 7 connectors, crypto vault, WASM sandbox |
| 1b | Bot Foundations | IN DESIGN — classical command routing, Telegram connector |
| 2a | Chat + AI | PLANNED — Discord, Signal, WhatsApp, Matrix, IRC, Slack, Nostr; AI adapters; sentinel |
| 2b | Desktop + Mobile + Safety | PLANNED — Tauri 2 shell, duress passphrase, panic wipe, travel mode |
| 3 | Veilid Mesh | PLANNED — P2P transport, E2E encrypted AI chat, distributed registry |

Full roadmap: [docs/ROADMAP.md](docs/ROADMAP.md)

---

## Ecosystem

- **[Rekindle](https://github.com/ScopeCreep-zip/Rekindle)** — Veilid-native decentralized gaming chat. Phase 3 transport wraps `rekindle-protocol`.
- **[Konductor](https://github.com/braincraftio/konductor)** — Nix flake framework for reproducible dev environments.
- **[Kalilix](https://github.com/ScopeCreep-zip/kalilix)** — Nix-based polyglot dev environment with security tooling.
- **[SpiritStream](https://github.com/ScopeCreep-zip/SpiritStream)** — Tauri desktop app patterns.

---

## Documentation

| New to Springtale? | Know what you're looking for? | Want to contribute? |
|---|---|---|
| [Learning path](docs/guide/) | [CLI reference](docs/reference/cli.md) | [Contributing guide](docs/contributing/) |
| [Architecture guide](docs/guide/architecture.md) | [API reference](docs/reference/api.md) | [Design decisions](docs/contributing/design-decisions.md) |
| [Security guide](docs/guide/security.md) | [Config reference](docs/reference/configuration.md) | [Adding a connector](docs/contributing/adding-a-connector.md) |
| [Glossary](docs/GLOSSARY.md) | [Connector reference](docs/reference/connectors/) | [Full architecture spec](docs/current-arch/) |

---

## Contributing

We welcome security review, accessibility expertise, i18n, and code. Start with [docs/contributing/](docs/contributing/).

## License

[MIT](LICENSE) — ScopeCreep.zip, 2026
