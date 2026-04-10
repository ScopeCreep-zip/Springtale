# Springtale

Local-first, privacy-preserving automation platform for people whose
safety depends on privacy. Bots are the primary unit — connector
infrastructure first, AI consumer second. Everything works without AI.

**Target users:** Trans people, POC, activists, IPV survivors, immigrants —
people facing real surveillance, doxxing, deplatforming, and harassment.

## Build & Test

```bash
cargo build --workspace                    # build all
cargo test --workspace                     # test all
cargo clippy --workspace --all-targets -- -D warnings  # lint
cargo fmt --check                          # format check
cargo nextest run --workspace              # fast test runner (preferred)
cd tauri && pnpm build                     # frontend build
cd tauri/apps/desktop && pnpm tauri dev    # desktop dev
cd tauri/apps/dashboard && pnpm dev        # web dashboard dev
```

## Product Model — read first

**@.claude/rules/shared/product-model.md** — the bot-first model, how settings
are scoped (app → formation → bot), the colony UI vision, what not to do.

## Architecture

**Use `current-arch` — it supersedes `intended-arch` where they differ.**

- Full architecture: `docs/current-arch/ARCHITECTURE.md`
- Security model: `docs/current-arch/SECURITY.md`
- Colony v8 visual reference: `docs/intended-arch/springtale-colony-v8.html`
- Cooperation framework: `docs/intended-arch/COOPERATION.pdf`

## Core Constraints (non-negotiable)

1. **Security and privacy are constraints, not features.** Every decision evaluated against threat model.
2. **Built for the most vulnerable user.** Default-safe. Zero telemetry.
3. **`NoopAdapter` must work.** The entire platform operates without any AI plugged in.
4. **Secrets are types.** All sensitive values wrapped in `Secret<T>` from `secrecy`.
5. **No native-tls.** `rustls-tls` exclusively.
6. **Modules over inline.** All functions, types, error variants in named modules.
7. **Connectors are untrusted.** WASM sandbox, manifest signing, capability allow-list.
8. **Transport is swappable.** All inter-node comms through `Transport` trait.

## Workspace Structure

```
crates/       — Pure Rust library crates (no Tauri dependency)
connectors/   — First-party connector crates
apps/         — springtaled (daemon) + springtale-cli
tauri/        — Desktop shell + web dashboard (SolidJS + Tailwind)
sdk/          — TypeScript connector SDK
docs/         — Architecture, security, design references
```

## Dependency Rules

- All version pins at workspace root `Cargo.toml`.
- `thiserror` for library errors. `anyhow` only in app binaries.
- `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` in library crates.
- `#![forbid(unsafe_code)]` except `springtale-crypto` and `springtale-connector`.

## Competitive Context

- **Obsoletes OpenClaw** — 250K+ stars but 800+ malicious skills, CVE-2026-25253 RCE, no sandboxing.
- **Obsoletes NosytLabs** — framework makes ad-hoc unsandboxed MCP servers obsolete.
- **Phase 3 adds P2P** — E2E encrypted AI chat via Veilid, no server, no phone number.

## Rules

Rules are organized by domain and path-scoped:

- `.claude/rules/backend/` — Rust conventions, security, crate structure, connectors, testing
- `.claude/rules/frontend/` — SolidJS conventions, Tauri integration
- `.claude/rules/shared/` — Product model, git workflow

@.claude/rules/shared/product-model.md
@.claude/rules/backend/rust-conventions.md
@.claude/rules/backend/security.md
@.claude/rules/backend/crate-structure.md
@.claude/rules/backend/connector-guidelines.md
@.claude/rules/backend/testing.md
@.claude/rules/frontend/solidjs-conventions.md
