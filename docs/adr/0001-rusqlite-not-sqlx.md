# ADR 0001: rusqlite (with SQLite3MultipleCiphers) over sqlx

**Status:** Accepted
**Date:** 2026-03-28

## Context

Springtale needs a single-file embedded SQL store with encryption at
rest. The constraints:

- Encryption at rest must be transparent to query code — no per-column
  encrypt/decrypt scattered through the store layer.
- Encryption uses AEAD (ChaCha20-Poly1305 or AES-GCM), not the older
  block-cipher constructions that SQLite's built-in encryption (SEE)
  supports.
- Must be synchronous internally. The store is called from inside
  `tokio::task::spawn_blocking`; pushing async through every SQLite
  call adds complexity for no gain.
- Must compile to a single static binary. No dynamic SQLite link, no
  required system library.
- Must support WAL mode + foreign keys + ON CONFLICT.

## Decision

Use `rusqlite` against a vendored
[SQLite3MultipleCiphers](https://github.com/utelle/SQLite3MultipleCiphers)
build. The vendored shim lives in `crates/libsqlite3-sys-mc`.

## Consequences

Positive:

- Single-file, single-binary store with AEAD encryption-at-rest.
- Synchronous API matches the actual blocking nature of SQLite — no
  fake async wrapping a thread pool.
- ChaCha20-Poly1305 cipher matches our vault crypto (consistency).
- `PRAGMA key=…` lets the schema-apply machinery treat encrypted and
  plaintext databases identically modulo the key step.
- Vendoring means no system SQLite dependency at runtime.

Negative:

- Bigger binary (~3 MB extra from the vendored SQLite build).
- Compile-time bound: SQLite version is whatever the vendored shim
  pins. Bumping requires touching `libsqlite3-sys-mc`.
- No compile-time SQL verification like `sqlx::query!` would give us.
  Mitigated by typed wrapper functions in `springtale-store::backend`
  that we test thoroughly.

Locks in:

- Our schema is SQLite-flavoured. Switching to Postgres later is a
  rewrite, not a config change.
- We can't easily share a database between multiple `springtaled`
  processes. That's a deliberate Phase 3 constraint anyway.

## Alternatives considered

### Option A — `rusqlite` + SQLite3MultipleCiphers (picked)

Pros: matches every constraint above. Synchronous. AEAD. Single
binary. Familiar API.
Cons: enumerated above.

### Option B — `sqlx` with built-in SQLite support

Pros: compile-time SQL verification (`query!` macros). Async-native.
Cons: no AEAD encryption-at-rest unless we layer it ourselves. The
async layer wraps a blocking thread pool internally; it's not actually
async. Compile-time SQL verification requires a database to be present
at build time, which complicates CI and contributors.

Why we didn't pick it: the encryption requirement is the dealbreaker.
We'd be writing every column's encrypt/decrypt at the application
layer, which is the exact mess we wanted to avoid.

### Option C — `diesel` with SQLite

Pros: type-safe DSL. Migrations as a first-class concept.
Cons: no AEAD encryption-at-rest. DSL is opinionated; some of our
queries (recursive CTEs for cooperation history) don't map cleanly.
The `diesel-cli` adds another contributor dependency.

Why we didn't pick it: encryption again. Plus the migration story
later got replaced by declarative schema — see ADR 0006.

### Option D — Postgres

Pros: real concurrency, mature, encryption-at-rest via filesystem
or pgcrypto.
Cons: external service. Not a single binary. Now we're shipping a
deployment guide that says "install postgres, configure HBA, manage
SSL between daemon and DB". The Springtale target user runs this on
a laptop. Postgres is not that.

Why we didn't pick it: deployment complexity is fundamentally
incompatible with the local-first ethos.

### Option E — `sled`

Pros: pure-Rust, no FFI. Async-friendly.
Cons: not SQL. We'd reimplement query logic. The cooperation
framework has joins, recursive lookups, and ad-hoc analytical queries
during dashboard rendering. Doing those by hand against a KV store
is a worse trade than picking SQLite.

Why we didn't pick it: SQL is load-bearing for the cooperation
analytics path.

## References

- `crates/springtale-store/src/backend/sqlite/mod.rs` — wraps rusqlite
- `crates/libsqlite3-sys-mc/` — vendored SQLite3MultipleCiphers
- [SQLite3MultipleCiphers wiki](https://utelle.github.io/SQLite3MultipleCiphers/)
- `crates/springtale-store/src/schema/apply.rs` — schema-apply
- Related: ADR 0006 (declarative schema)
