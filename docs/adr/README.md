# Architecture Decision Records

Each ADR documents one architectural decision: what we chose, what we
considered, and why we picked the option we did. New ADRs go in numbered
files; existing ADRs are immutable once accepted (supersede with a new
ADR rather than editing).

This complements [`docs/contributing/design-decisions.md`](../contributing/design-decisions.md),
which is the one-page summary of trade-offs. ADRs are the long form.

## Index

| # | Title | Status | Date |
|---|---|---|---|
| [0001](0001-rusqlite-not-sqlx.md) | rusqlite (with SQLite3MultipleCiphers) over sqlx | Accepted | 2026-03-28 |
| [0002](0002-wasmtime-not-wasmer.md) | Wasmtime for the connector sandbox | Accepted | 2026-03-28 |
| [0003-rustls-only-no-native-tls](0003-rustls-only-no-native-tls.md) | rustls only; native-tls banned | Accepted | 2026-03-28 |
| [0004](0004-secrecy-crate-for-secrets.md) | Use `secrecy` crate's `Secret<T>` for all credentials | Accepted | 2026-03-28 |
| [0005](0005-cooperation-as-separate-crate.md) | Extract cooperation framework to its own crate | Accepted | 2026-04-10 |
| [0006](0006-declarative-schema-over-migrations.md) | Declarative schema with `PRAGMA user_version`, not incremental migrations | Accepted | 2026-05-01 |
| [0007](0007-axum-over-actix.md) | Axum 0.8 for the HTTP API | Accepted | 2026-03-28 |
| [0008](0008-tauri-over-electron.md) | Tauri 2 for desktop, not Electron | Accepted | 2026-03-28 |
| [0009](0009-veilid-for-phase-3.md) | Veilid for Phase 3 P2P transport | Accepted | 2026-03-28 |
| [0010](0010-default-deny-approval-gate.md) | `DefaultDenyApprovalGate` as the headless default | Accepted | 2026-05-05 |

## Template

When you propose a new decision:

```markdown
# ADR NNNN: <Short title in imperative mood>

**Status:** Proposed | Accepted | Superseded by ADR-XXXX | Deprecated
**Date:** YYYY-MM-DD
**Deciders:** @username, @username
**Consulted:** anyone who reviewed but didn't decide
**Informed:** anyone we told after

## Context

What problem is this solving? What constraints are in play? What
forced this decision now?

## Decision

What did we pick? One paragraph max.

## Consequences

What does this lock in? What does it preclude? Both positive and
negative.

## Alternatives considered

Each option as a subsection with its own pros and cons.

### Option A — what we picked

Pros: …
Cons: …

### Option B — runner-up

Pros: …
Cons: …
Why we didn't pick it: …

### Option C — viable but ruled out

…

## References

- Existing code that this decision shapes
- External research, blog posts, RFCs
- Related ADRs
```

## Why ADRs and not just a single design-decisions.md

We have both. `design-decisions.md` is the cheat-sheet — three columns,
one row per trade-off. ADRs are the full reasoning, including
alternatives we rejected.

The cheat-sheet is for someone reading the repo and asking "wait, why
sqlite?". The ADR is for someone proposing to switch to sqlx and needing
to know what we already considered.

If you propose changing one of these decisions in a PR or discussion,
**read the ADR first**. We may have already discarded your idea, in
which case you can either bring new information or save your time.
