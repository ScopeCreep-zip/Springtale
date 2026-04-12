# Contributing

Springtale is built for marginalized communities by people who understand the stakes. We welcome contributions across the board — and we value security review, accessibility expertise, and i18n contributions as much as code.

## 1. What We Value

- **Security review** — Audit the crate code, find holes in the threat model, test the sandbox
- **Accessibility** — Screen reader support, keyboard navigation, i18n, RTL, older device support
- **Connector development** — New service integrations (see [adding-a-connector.md](adding-a-connector.md))
- **Documentation** — Clarify guides, fix examples, improve the glossary
- **Bug fixes** — Especially in error handling, edge cases, and security boundaries

## 2. Getting Started

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
cargo build --workspace
cargo nextest run --workspace
```

Then read [guide/architecture.md](../guide/architecture.md) to understand how the pieces fit together.

```
   clone ───► build ───► nextest ───► read guide/architecture.md
     │                      │                    │
     │                      │                    └─► pick an area:
     │                      │                        • connector
     │                      │                        • bot/cooperation
     │                      │                        • security
     │                      │                        • docs / a11y / i18n
     │                      │
     │                      └─► green? open a draft PR
     │
     └─► first time? konductor dev shell: `direnv allow`
```

*Fig. 1. Contributor onboarding loop.*

## 3. Phase Discipline

Springtale ships in phases. **Do not build Phase N+1 features while implementing Phase N.**

Stubs and trait definitions for future phases are fine. Implementations are not. If you're unsure whether something belongs in the current phase, check [ROADMAP.md](../ROADMAP.md) or open an issue.

## 4. Code Conventions

The workspace enforces strict conventions:

- **Module structure** — `lib.rs` is a table of contents only. No functions, types, or impls. Everything lives in named modules.
- **Error handling** — `thiserror` in libraries, `anyhow` only in app binaries. No `unwrap()`, `expect()`, or `panic!()` in library code.
- **Secrets** — All credentials wrapped in `Secret<String>`. Every `.expose_secret()` annotated with `// SECURITY:`.
- **TLS** — `rustls-tls` exclusively. `native-tls` banned.
- **Unsafe** — `#![forbid(unsafe_code)]` on all crates except `springtale-crypto` and `springtale-connector` (audited blocks only).

Full conventions in `.claude/rules/`: `rust-conventions.md`, `security.md`, `crate-structure.md`, `testing.md`, `connector-guidelines.md`.

## 5. CI Requirements

Every PR must pass all checks before merge:

| Check | Command | What it catches |
|-------|---------|----------------|
| Format | `cargo fmt --check` | Style inconsistencies |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Bugs, anti-patterns |
| Test | `cargo nextest run --workspace` | Regressions |
| Doc tests | `cargo test --doc` | Broken doc examples |
| Licenses | `cargo deny check` | Unapproved licenses, advisories |
| Vulnerabilities | `cargo audit` | Known CVEs in dependencies |
| Secrets | `gitleaks` | Accidentally committed credentials |

No exceptions. No `--no-verify`. Fix the issue, don't bypass the check.

---

## References

- [1] Architecture guide: [guide/architecture.md](../guide/architecture.md)
- [2] Design decisions: [design-decisions.md](design-decisions.md)
- [3] Adding a connector: [adding-a-connector.md](adding-a-connector.md)
- [4] Roadmap: [ROADMAP.md](../ROADMAP.md)
