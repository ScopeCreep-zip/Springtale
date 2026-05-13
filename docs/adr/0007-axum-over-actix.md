# ADR 0007: Axum 0.8 for the HTTP API

**Status:** Accepted
**Date:** 2026-03-28

## Context

The daemon serves an HTTP API for: the CLI, the Tauri desktop, the
web dashboard, webhook ingestion, SSE streams (events + canvas +
cooperation), the MCP bridge surface, and possibly third-party
clients.

Requirements:

- Async, tokio-based (matches the rest of the workspace).
- Middleware composition that doesn't require runtime magic.
- Typed extractors (so route handlers can declare exactly what they
  need).
- SSE support (we have at least three SSE endpoints).
- No global state shenanigans.
- Pure Rust, rustls-compatible (see ADR 0003).

## Decision

Use [Axum](https://docs.rs/axum) 0.8 from the tokio team, backed by
hyper + tower middleware. Routes live in `apps/springtaled/src/api/`,
one module per concern.

## Consequences

Positive:

- Tower middleware composition is familiar to anyone who's used the
  rest of the tokio ecosystem.
- Axum's typed extractors catch missing-state bugs at compile time.
- SSE works out of the box (`axum::response::Sse`).
- Strong fit for our existing `tracing-subscriber` + `tower-http`
  setup.
- Compatible with `tower_http::limit`, `tower_http::cors`, etc.
- No global registry. Routes attach to an `axum::Router`, state
  attaches to that router. Clean.
- Maintained by the tokio team; same release cadence as our async
  runtime.

Negative:

- Axum's API has churned across versions. We've bumped from 0.6 →
  0.7 → 0.8 in the project's lifetime, each requiring breaking
  changes in handlers.
- Macro-free extraction means longer function signatures than some
  frameworks. Trade-off for explicitness.
- Performance is in the same band as actix-web; not a meaningful
  win or loss.

Locks in:

- Tower-style middleware. Anything we want to write as a middleware
  layer must implement `tower::Layer`.
- Hyper as the underlying server.
- `axum::extract` patterns for state, body, query, path.

## Alternatives considered

### Option A — Axum 0.8 (picked)

Pros and cons enumerated above.

### Option B — actix-web

Pros: mature, fast, large community.
Cons: actor model adds runtime conceptual overhead. Not as cleanly
integrated with tower / hyper / tokio. The maintainer story has been
turbulent. Some unsafe in the framework.

Why we didn't pick it: less alignment with the tokio ecosystem we're
already in. Actor model would be the only place in our codebase
using actors.

### Option C — warp

Pros: filter combinator approach, type-driven, async.
Cons: the type-level filter composition produces enormous error
messages on mistake. Smaller community. Less maintained.

Why we didn't pick it: developer ergonomics. Compile errors that take
30 seconds to read aren't worth the filter abstraction.

### Option D — Roll our own on top of hyper

Pros: total control.
Cons: we'd be reimplementing axum, badly. Plus the maintenance.

Why we didn't pick it: not where our value-add lives.

### Option E — rocket

Pros: macro-heavy, ergonomic.
Cons: macro magic obscures the request flow. Async story has been
weak historically. Less integration with the rest of the ecosystem.

Why we didn't pick it: too much magic for a security-sensitive
codebase. We'd rather read explicit handler signatures.

## References

- `apps/springtaled/src/api/mod.rs` — router construction
- `apps/springtaled/src/api/*.rs` — per-concern modules
- `apps/springtaled/src/api/cooperation_stream.rs` — example SSE handler
- [Axum docs](https://docs.rs/axum)
- [Tower middleware](https://docs.rs/tower)
