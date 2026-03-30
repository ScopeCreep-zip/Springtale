---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Security Rules (Non-Negotiable)

## Secret Handling
- ALL credentials, API keys, tokens wrapped in `Secret<T>` from `secrecy` crate.
- `Secret<T>` created at config parse time, never unwrapped except at precise call site.
- Every `.expose_secret()` call annotated: `// SECURITY: expose needed for X`
- No `Secret<T>` in structs deriving `Debug` without `secrecy` redacted display.
- `zeroize` on drop for all sensitive values.

## TLS
- `rustls-tls` exclusively. No `native-tls`, no OpenSSL.
- `native-tls` banned via `Cargo.toml [patch]` stub.
- TLS certificate validation NEVER disabled in any code path.
- All `reqwest` clients: `rustls-tls` feature only.

## Unsafe Code
- `#![forbid(unsafe_code)]` on: core, transport, scheduler, store, ai, mcp, bot, sentinel
- `#![deny(unsafe_code)]` on: crypto, connector (audited unsafe blocks only)
- Every `unsafe` block has a `// SAFETY:` comment explaining the invariant.

## WASM Sandbox (Connectors)
- Community connectors run in Wasmtime sandbox.
- Fuel metering: 10M instructions per invocation.
- Memory limit: 64MB (1024 pages).
- Wall-clock timeout: 30s.
- `NetworkOutbound` capability: exact host match, no wildcards.

## Manifest Signing
- Ed25519 signatures on all connector manifests.
- Verify signature before load. Verify on every subsequent load (hash check).
- `ShellExec` capability triggers blocking approval — cannot be bypassed.

## Database
- No raw SQL strings. All queries through `springtale-store` crate.
- SQLite: `rusqlite` with bundled SQLite, WAL mode.
- PostgreSQL (optional): `sqlx::query_as!` macro only (compile-time verified).
- Database file permissions: `0o600`.

## Network
- Management API binds `127.0.0.1` by default. Warn on `0.0.0.0`.
- HMAC bearer tokens for API auth.
- Rate limiting via `tower-http::limit`.
- No secrets in URLs, query params, or error messages.
