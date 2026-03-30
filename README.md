<p align="center">
  <img src="docs/logo/Bug.png" alt="Springtale mascot" width="200">
</p>

# Springtale

A local-first, privacy-preserving automation platform built for people whose safety depends on privacy. Connector infrastructure first, AI consumer second.

Springtale provides a typed, signed, sandboxed connector framework that lets you automate across services — chat platforms, APIs, filesystems, search engines — without trusting any connector with your data, your credentials, or your identity.

Everything works without AI. Plug in a local model or API key if you want. Unplug it and nothing breaks.

## 1. Introduction

Existing automation and agent platforms run untrusted code with full machine access, store secrets in plaintext, phone home to central servers, and require phone numbers or emails that link your identity across contexts. If you've been doxxed, surveilled, deplatformed, or worse — those aren't acceptable tradeoffs.

Springtale is built from the ground up with a different set of priorities:

- **No telemetry.** Not opt-out. Not anonymized. It doesn't exist.
- **No phone number.** No email. No real name. Identity is a keypair.
- **No central server.** Self-hosted in Phase 1-2. Fully P2P in Phase 3.
- **No trust required.** Community connectors run in a WASM sandbox with signed manifests and declared capabilities. They cannot access anything you don't approve.
- **No AI dependency.** The default adapter is `NoopAdapter`. Rules, automations, and bot commands work without any AI.

---

## 2. System Architecture

Springtale is a Rust workspace of 17 crates — 8 libraries, 7 connectors, and 2 applications. ~20,000 lines of Rust.

### 2.1. Workspace

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
          │   ┌──────────┐   ┌──────────┐   ┌────────────────┐       │
          │   │   mcp    │   │    ai    │   │   scheduler    │       │
          │   │ protocol │   │ adapter  │   │ cron, watcher, │       │
          │   │ bridge   │   │ + noop   │   │ jobs, retry    │       │
          │   └────┬─────┘   └────┬─────┘   └───────┬────────┘       │
          │        │              │                  │               │
          │        v              v                  v               │
          │   ┌─────────────────────────────────────────────────┐    │
          │   │                 connector                       │    │
          │   │  trait, registry, manifest, capability, wasm    │    │
          │   └────────────┬────────────────────┬───────────────┘    │
          │                │                    │                    │
          │                v                    v                    │
          │   ┌──────────────────┐   ┌──────────────────┐            │
          │   │      store       │   │      crypto      │            │
          │   │  SQLite backend  │   │ Ed25519, vault,  │            │
          │   │  schema, queries │   │ signatures       │            │
          │   └────────┬─────────┘   └──────────────────┘            │
          │            │                                             │
          │            v                                             │
          │   ┌──────────────────┐   ┌──────────────────┐            │
          │   │       core       │   │    transport     │            │
          │   │  rule engine,    │   │ Unix socket      │            │
          │   │  pipeline,       │   │ (← crypto)       │            │
          │   │  router          │   │                  │            │
          │   │  (zero deps)     │   │                  │            │
          │   └──────────────────┘   └──────────────────┘            │
          │                     Library Crates                       │
          └────────────────────────────────────────────────────────-─┘
```

*Fig. 1. Crate dependency graph. Arrows point from dependent to dependency. `core` and `crypto` have zero internal dependencies.*

### 2.2. How It Works

When something happens — a Kick stream goes live, a file changes, a cron timer fires — the event flows through trigger matching, condition evaluation, pipeline processing, and connector dispatch:

```
  External Service          springtaled                     Connector
       │                       │                               │
       │  webhook/poll/watch   │                               │
       ├──────────────────────>│                               │
       │                       │  1. match trigger type        │
       │                       │  2. evaluate conditions       │
       │                       │  3. run pipeline stages       │
       │                       │  4. enqueue job               │
       │                       │  5. dispatch ────────────────>│
       │                       │     (capability check first)  │
       │                       │<──── result ──────────────────┤
       │                       │  6. log event                 │
```

*Fig. 2. Event-driven rule evaluation pipeline. See [guide/architecture.md](docs/guide/architecture.md) for the full flow diagram.*

### 2.3. Phases

**TABLE I. PHASE STATUS**

| Phase | Name | Status | Key Deliverables |
|-------|------|--------|-----------------|
| 1a | Framework + Connectors | **COMPLETE** | Daemon, CLI, 8 crates, 7 connectors, crypto vault, WASM sandbox |
| 1b | Bot Foundations | IN DESIGN | `springtale-bot`, command routing, `connector-telegram` |
| 2a | Chat + AI | PLANNED | 7 chat connectors, AI adapters, sentinel, `HttpTransport` |
| 2b | Desktop + Mobile + Safety | PLANNED | Tauri 2, visual rule builder, duress/panic/travel mode |
| 3 | Veilid Mesh | PLANNED | P2P transport, distributed registry, E2E encrypted AI chat |

Full roadmap: [docs/ROADMAP.md](docs/ROADMAP.md)

---

## 3. Security Model

Security and privacy are constraints, not features. Every decision is evaluated against the threat model for the most vulnerable user [1].

### 3.1. Defense in Depth

```
  ┌───────────────────────────────────────────────────────────┐
  │  Zero Telemetry — nothing leaves your device              │
  ├───────────────────────────────────────────────────────────┤
  │  Transport Encryption — rustls-tls only, OpenSSL banned   │
  ├───────────────────────────────────────────────────────────┤
  │  Vault Encryption — XChaCha20-Poly1305 + Argon2id KDF     │
  ├───────────────────────────────────────────────────────────┤
  │  WASM Sandbox — 10M fuel, 64MB memory, 30s timeout        │
  ├───────────────────────────────────────────────────────────┤
  │  Capability Model — exact-host matching, toxic pair block │
  ├───────────────────────────────────────────────────────────┤
  │  Manifest Signing — Ed25519, verify on every load         │
  ├───────────────────────────────────────────────────────────┤
  │  Secret<T> — cannot log, clone, serialize; zeroed on drop │
  ├───────────────────────────────────────────────────────────┤
  │  Supply Chain — cargo-deny, cargo-audit, gitleaks         │
  └───────────────────────────────────────────────────────────┘
```

*Fig. 3. Eight independent security layers. Compromise of one doesn't cascade.*

### 3.2. Connector Isolation

```
  ┌─ Native (in-process) ─────────────────────────────────────-┐
  │  7 first-party connectors                                  │
  │  Trust: HIGH — audited, signed by Springtale team          │
  │  Isolation: capability-checked at runtime                  │
  └────────────────────────────────────────────────────────────┘

  ┌─ WASM (sandboxed) ────────────────────────────────────────┐
  │  ┌──────────────────────────────────────────────────────┐ │
  │  │  Wasmtime: 10M instr │ 64MB mem │ 30s timeout        │ │
  │  │  Host API: only declared capabilities exposed        │ │
  │  └──────────────────────────────────────────────────────┘ │
  │  Trust: LOW — community-authored, untrusted               │
  └───────────────────────────────────────────────────────────┘
```

*Fig. 4. Connector trust boundary. Native connectors run in-process. WASM connectors are sandboxed.*

### 3.3. Cryptographic Primitives

**TABLE II. CRYPTOGRAPHIC ALGORITHMS**

| Algorithm | Purpose | Crate |
|-----------|---------|-------|
| Ed25519 | Node identity, manifest signing, capability tokens | `ed25519-dalek` |
| XChaCha20-Poly1305 | Vault encryption (secrets at rest) | `chacha20poly1305` |
| Argon2id | Key derivation from passphrase | `argon2` |
| HMAC-SHA256 | API bearer tokens, webhook verification | `hmac`, `sha2` |
| SHA-256 | Content hashing, integrity checks | `sha2` |

Full threat model, OWASP ASVS mapping, MITRE ATT&CK mapping: [docs/current-arch/SECURITY.md](docs/current-arch/SECURITY.md)

---

## 4. Connectors

Seven first-party connectors ship with Phase 1a:

**TABLE III. FIRST-PARTY CONNECTORS**

| Connector | Platform | Triggers | Actions | Auth |
|-----------|----------|----------|---------|------|
| `connector-kick` | Kick (streaming) | 4 (webhook) | 3 | OAuth 2.1 PKCE |
| `connector-presearch` | Presearch (search) | 0 | 2 (cached) | API key |
| `connector-bluesky` | Bluesky/ATProto | 4 (Jetstream) | 4 | Session auth |
| `connector-github` | GitHub | 4 (webhook) | 3 | PAT |
| `connector-filesystem` | Local filesystem | 3 (watcher) | 3 | None (path allow-list) |
| `connector-shell` | Shell commands | 0 | 1 | None (command allow-list) |
| `connector-http` | Generic HTTP | 0 | 2 | None (host allow-list) |

Any connector automatically becomes an MCP server via `springtale-mcp`. One framework, not N hand-written servers.

```toml
# rules/kick-to-bluesky.toml — no AI required
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

Full connector documentation: [docs/guide/connectors.md](docs/guide/connectors.md) | Per-connector reference: [docs/reference/connectors/](docs/reference/connectors/)

---

## 5. CLI and API

### 5.1. CLI Commands

```
springtale init                           create vault + database
springtale server start                   start daemon inline
springtale connector install <manifest>   install from TOML manifest
springtale connector list                 list installed connectors
springtale connector enable/disable <n>   toggle connector
springtale connector remove <name>        remove connector
springtale rule add <file>                add rule from TOML/JSON
springtale rule list                      list all rules
springtale rule toggle <id>               toggle enabled/disabled
springtale rule run <id>                  dry-run evaluation
springtale events --limit 50              query event log
```

Full CLI reference: [docs/reference/cli.md](docs/reference/cli.md)

### 5.2. Management API

14 endpoints on `127.0.0.1:8080` (configurable). Bearer token auth via HMAC-SHA256.

**TABLE IV. API ENDPOINTS**

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | No | Liveness probe |
| GET | `/ready` | No | Readiness probe |
| GET | `/connectors` | Yes | List connectors |
| POST | `/connectors/install` | Yes | Install connector |
| DELETE | `/connectors/{name}` | Yes | Remove connector |
| POST | `/connectors/{name}/enable` | Yes | Enable connector |
| POST | `/connectors/{name}/disable` | Yes | Disable connector |
| GET | `/rules` | Yes | List rules |
| POST | `/rules` | Yes | Create rule |
| PUT | `/rules/{id}` | Yes | Update rule |
| DELETE | `/rules/{id}` | Yes | Delete rule |
| POST | `/rules/{id}/run` | Yes | Dry-run rule |
| GET | `/events` | Yes | Query event log |
| POST | `/webhook/{connector}/{trigger}` | Yes | Receive webhook |

Full API reference: [docs/reference/api.md](docs/reference/api.md)

---

## 6. Getting Started

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
cargo build --workspace
cargo run --bin springtale-cli -- init
cargo run --bin springtale-cli -- server start
```

With Nix: `direnv allow` loads all tools. With Docker: `docker compose up -d`.

Full quickstart with a worked example: [docs/QUICKSTART.md](docs/QUICKSTART.md)

---

## 7. CI/CD

```
  Pull Request
       │
       v
  ┌──────────────────────────────────────────────┐
  │                CI Pipeline                   │
  │                                              │
  │  ┌─────────┐  ┌─────────┐  ┌──────────────┐  │
  │  │  fmt    │  │ clippy  │  │    test      │  │
  │  │         │  │ (SAST)  │  │ nextest +    │  │
  │  │         │  │         │  │ doc tests    │  │
  │  └────┬────┘  └────┬────┘  └──────┬───────┘  │
  │       │            │              │          │
  │  ┌────v────┐  ┌────v────┐  ┌────-─v───────┐  │
  │  │  deny   │  │  audit  │  │  gitleaks    │  │
  │  │ license │  │ RustSec │  │  secrets     │  │
  │  │ +advisory│ │         │  │  detection   │  │
  │  └────┬────┘  └────┬────┘  └─────┬────────┘  │
  └───────┼────────────┼─────────────┼───────────┘
          v            v             v
       ALL PASS ──────────────> Merge Allowed
```

*Fig. 5. CI pipeline. All checks must pass. No merges without green.*

---

## 8. Ecosystem

Springtale draws from and contributes to a constellation of projects:

- **[Rekindle](https://github.com/ScopeCreep-zip/Rekindle)** — Veilid-native decentralized gaming chat. Phase 3 transport wraps `rekindle-protocol`.
- **[Konductor](https://github.com/braincraftio/konductor)** — Nix flake framework for reproducible dev environments.
- **[Kalilix](https://github.com/ScopeCreep-zip/kalilix)** — Nix-based polyglot dev environment with security tooling.
- **[SpiritStream](https://github.com/ScopeCreep-zip/SpiritStream)** — Tauri desktop app patterns.

---

## 9. Documentation

**TABLE V. DOCUMENTATION MAP**

| Path | Audience | Content |
|------|----------|---------|
| [docs/guide/](docs/guide/) | Newcomers, students | How things work — architecture, security, connectors, rules |
| [docs/reference/](docs/reference/) | Developers | Exact specs — API, CLI, config, per-connector details |
| [docs/contributing/](docs/contributing/) | Contributors | Design decisions, connector authoring, code conventions |
| [docs/QUICKSTART.md](docs/QUICKSTART.md) | Everyone | Zero to running in 5 minutes |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Everyone | Phase status and delivery plan |
| [docs/GLOSSARY.md](docs/GLOSSARY.md) | Everyone | ~40 technical terms defined |
| [docs/current-arch/](docs/current-arch/) | Auditors, security reviewers | Full architecture spec, threat model, compliance mappings |

---

## 10. Status

Phase 1a complete (~20,000 lines of Rust). Phase 1b in design.

Architecture docs audited and validated. See [docs/current-arch/](docs/current-arch/) for the full specification.

## 11. Contributing

We welcome security review, accessibility expertise, i18n, and code contributions. Start with [docs/contributing/](docs/contributing/).

## 12. License

[MIT](LICENSE) — ScopeCreep.zip, 2026

---

## References

- [1] Full architecture: [docs/current-arch/ARCHITECTURE.md](docs/current-arch/ARCHITECTURE.md)
- [2] Security model: [docs/current-arch/SECURITY.md](docs/current-arch/SECURITY.md)
- [3] Rekindle P2P protocol: [docs/current-arch/rekindle-architecture.md](docs/current-arch/rekindle-architecture.md)
- [4] Audit findings: [docs/current-arch/AUDIT-NOTES.md](docs/current-arch/AUDIT-NOTES.md)
