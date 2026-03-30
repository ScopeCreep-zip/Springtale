# Springtale

Local-first, privacy-preserving automation platform built for people whose
safety depends on privacy. Rust workspace. Connector infrastructure first,
AI consumer second.

**Target users:** Trans people, POC, activists, IPV survivors, immigrants —
people facing real surveillance, doxxing, deplatforming, and harassment.
Every decision evaluated from the perspective of the most vulnerable user.

## Build & Test

```bash
cargo build --workspace                    # build all
cargo test --workspace                     # test all
cargo clippy --workspace --all-targets -- -D warnings  # lint (warnings = errors)
cargo fmt --check                          # format check
cargo nextest run --workspace              # fast test runner (preferred)
```

## Architecture (read before writing code)

**Use `current-arch` — it supersedes `intended-arch` where they differ.**

- Full architecture: `docs/current-arch/ARCHITECTURE.md`
- Security model: `docs/current-arch/SECURITY.md`
- Rekindle P2P spec: `docs/current-arch/rekindle-architecture.md`
- Audit findings: `docs/current-arch/AUDIT-NOTES.md`
- Change log: `docs/current-arch/CHANGELOG.md`

Original (pre-audit) docs preserved in `docs/intended-arch/` for reference.

## Phase Roadmap — know what phase you're building

- **Phase 1a**: Framework + Connectors (springtaled, CLI, SQLite, LocalTransport, NoopAdapter, 7 connectors)
- **Phase 1b**: Bot Foundations (springtale-bot, classical command routing, connector-telegram)
- **Phase 2a**: OpenClaw Parity (HttpTransport, AI adapters, sentinel, chat connectors, recursive pipelines)
- **Phase 2b**: Desktop + Mobile + Safety (Tauri 2, duress/panic, travel mode, app disguise, accessibility)
- **Phase 3**: Veilid Mesh (VeilidTransport via rekindle-protocol, distributed registry)

**Do not build Phase N+1 features while implementing Phase N.**
Stubs and trait definitions for future phases are fine. Implementations are not.

## Core Constraints (non-negotiable)

1. **Security and privacy are constraints, not features.** Every decision evaluated against threat model (§2.1-2.9).
2. **Built for the most vulnerable user.** Default-safe. Metadata-leaking features off by default. Zero telemetry.
3. **`NoopAdapter` must work.** The entire platform operates correctly without any AI plugged in.
4. **Secrets are types.** All sensitive values wrapped in `Secret<T>` from `secrecy`. Memory zeroed on drop via `zeroize`.
5. **No native-tls.** `rustls-tls` exclusively. `native-tls` banned via Cargo.toml patch.
6. **Modules over inline.** All functions, types, error variants in named modules. No free-floating impl blocks at crate root.
7. **Connectors are untrusted.** WASM sandbox, manifest signing, capability allow-list.
8. **Transport is swappable.** All inter-node comms through `Transport` trait. No concrete transport escapes the module.

## Workspace Structure

- `crates/` — Pure Rust library crates (no Tauri dependency)
- `connectors/` — First-party connector crates
- `apps/` — springtaled (daemon) + springtale-cli
- `tauri/` — Desktop shell (Phase 2b)
- `sdk/` — TypeScript connector SDK (jco componentize → wasm32-wasip2)

## Dependency Rules

- All version pins at workspace root `Cargo.toml`. No crate specifies its own version for shared deps.
- Bounded version ranges only (e.g., `"42"` not `">=42.0.0"`). No unbounded `>=` pins.
- `thiserror` for library error types. `anyhow` only in app binaries. Transport trait uses `TransportError`.
- `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` in all library crates.
- `#![forbid(unsafe_code)]` on all crates except `springtale-crypto` and `springtale-connector` (audited unsafe only).

## Competitive Context

- **Phase 1a obsoletes NosytLabs' approach** — framework makes ad-hoc unsandboxed MCP servers obsolete.
- **Phase 2a obsoletes OpenClaw** — 250K+ stars but 800+ malicious skills in ClawHub, CVE-2026-25253 RCE, no sandboxing.
- **Phase 3 adds what no centralized platform can match** — E2E encrypted P2P AI chat via Veilid, no server, no phone number.

@.claude/rules/rust-conventions.md
@.claude/rules/security.md
@.claude/rules/crate-structure.md
@.claude/rules/connector-guidelines.md
@.claude/rules/testing.md
