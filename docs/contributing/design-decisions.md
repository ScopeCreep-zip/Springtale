# Design Decisions

Why Springtale makes the choices it does. Each section covers what we chose, what we considered, and why.

## 1. Why Rust

**Chose:** Rust (stable, edition 2024)
**Considered:** Go, Python, TypeScript

Rust gives us memory safety without a garbage collector, a mature async ecosystem (tokio), first-class WASM compilation targets, and strong type-level guarantees (like `Secret<T>` wrapping credentials at compile time). The borrow checker catches entire classes of bugs — use-after-free, data races, dangling references — before the code ships.

The tradeoff: steeper learning curve and slower compile times. We accept this because security is a constraint, not a feature — and Rust's guarantees align with that constraint better than any alternative.

Go was considered for its simplicity and fast compiles, but lacks the type-level security guarantees we need (no equivalent of `Secret<T>`, no WASM sandbox story). Python/TypeScript were non-starters for a security-critical runtime.

## 2. Why Wasmtime over V8/Wasmer

**Chose:** Wasmtime (v43+, Bytecode Alliance)
**Considered:** V8, Wasmer, wasm3

Wasmtime has the strongest security audit history of any WASM runtime. It supports fuel metering (instruction budgets), WASI Preview 2, and is maintained by the Bytecode Alliance (Mozilla, Fastly, Intel, Microsoft).

Fuel metering is critical — it lets us kill a community connector that's stuck in an infinite loop after exactly 10 million instructions, not "sometime after 30 seconds." Memory limits (64MB / 1024 WASM pages) prevent a connector from exhausting the host.

V8 is a JavaScript engine that happens to run WASM — it's not designed as a sandbox runtime. Wasmer has had fewer security audits. wasm3 is an interpreter (slow) and doesn't support WASI-P2.

**Known risk:** Wasmtime has had CVEs (buffer overflow, DoS). We pin to >=42.0.0 per CVE-2026-27204 and track advisories via `cargo-audit`.

## 3. Why Ed25519 over RSA

**Chose:** Ed25519 (via `ed25519-dalek`)
**Considered:** RSA, ECDSA (P-256)

Ed25519 keys are 32 bytes (vs RSA's 256+ bytes). Signing and verification are ~10x faster than RSA-2048. The `ed25519-dalek` crate is widely audited and used in the Rust ecosystem.

RSA is still used where external services require it (Kick's webhook verification uses RSA), but all internal signing — manifest signatures, node identity, capability tokens — uses Ed25519.

ECDSA (P-256) was considered but has a more complex implementation surface and historical issues with nonce generation. Ed25519 uses deterministic nonces, eliminating that class of vulnerability.

## 4. Why SQLite over PostgreSQL

**Chose:** SQLite (via `rusqlite`, bundled)
**Considered:** PostgreSQL, Sled, plain files

Springtale is local-first. Users should be able to run it on a Raspberry Pi, a laptop, or a phone without installing a database server. SQLite is:

- Zero deployment — it's compiled into the binary
- ACID-compliant with WAL mode for concurrent reads
- Fast for the workload (single writer, many readers)
- Portable — the database is a single file you can back up by copying

PostgreSQL is available as an optional backend (via `sqlx`) for server-mode deployments in Phase 2, but SQLite is the default and the only requirement.

Sled was considered but has an uncertain maintenance future. Plain files lack transactions and queries.

## 5. Why rustls over OpenSSL/native-tls

**Chose:** `rustls-tls` exclusively
**Considered:** OpenSSL (via `native-tls`), ring directly

OpenSSL is a C library with a long history of CVEs (Heartbleed, etc.). `native-tls` delegates to the platform's TLS stack, which varies between operating systems and is hard to audit consistently.

rustls is written in pure Rust, has been formally verified (for key parts), and produces consistent behavior across all platforms. No C dependencies in the TLS stack.

We enforce this at three levels:
1. `deny.toml` bans `native-tls`, `openssl`, and `openssl-sys`
2. `vendor/native-tls-stub/` provides a fake `native-tls` crate that prevents accidental transitive dependencies
3. All `reqwest` clients use the `rustls-tls` feature flag

## 6. Why TOML for Rules

**Chose:** TOML
**Considered:** YAML, JSON, custom DSL

TOML is unambiguous. The Norway problem (YAML interprets `NO` as boolean `false`) doesn't exist. Types are explicit. Nesting is clear. It's human-readable and human-writable.

JSON was considered but lacks comments and is verbose for hand-authoring. YAML's implicit typing has caused real security incidents. A custom DSL would require a parser and add learning overhead.

Rules are the primary user-facing authoring surface — they need to be easy to write, easy to read, and impossible to misparse.

## 7. Why NoopAdapter as Default

**Chose:** `NoopAdapter` (returns "no AI configured") as the default AI adapter
**Considered:** Requiring an AI adapter at startup

The entire platform must work without AI. Rules, connectors, scheduling, the bot framework, the CLI — everything functions with `NoopAdapter`. AI is a pipeline action you can optionally add, not a dependency you must configure.

This matters because:
- Not everyone has access to AI APIs (cost, availability, censorship)
- Not everyone wants AI processing their data
- Rules and automations are deterministic and predictable without AI
- It proves the architecture is sound when AI is absent

## 8. Why Strict Module Structure

**Chose:** `lib.rs` contains only `pub mod` declarations and re-exports. Everything else lives in named modules.
**Considered:** Allowing inline types/functions in `lib.rs`

In a 10-crate workspace with 20K+ lines of Rust, you need to find things fast. When `lib.rs` is a table of contents, you see the full public API shape at a glance. When you open a module file, you see one focused concern.

The rule: no functions, no types, no impl blocks, no constants in `lib.rs`. Every public surface lives in a named module. No free-floating code at crate root.

The tradeoff: more files, deeper directory trees, more `pub use` re-exports. We accept this because navigability at scale matters more than convenience for small changes.

## 9. Known Tradeoffs

Not everything is ideal. These are risks we've accepted with eyes open:

**TABLE I. KNOWN TRADEOFFS**

| Risk | Severity | Details |
|------|----------|---------|
| `rmcp` version mismatch | HIGH | The MCP Rust SDK (`rmcp`) had a version disconnect between what's published and what's documented. We pin to v1.x and track closely. |
| `jco` WASM overhead | MEDIUM | TypeScript → WASM via `jco componentize` adds 3-5MB per connector due to the bundled JS engine. Acceptable for community connectors, not for first-party. |
| Veilid maturity | MEDIUM | Veilid is v0.4.x. Phase 3 is gated on production stability. The `VeilidTransport` stub is ready, but we won't ship it until Veilid is stable. |
| WASM binary size | LOW | Even Rust WASM connectors are ~1-2MB. Acceptable for local installation, but limits distribution via DHT in Phase 3. |
| Compile times | LOW | Full workspace rebuild: ~2-3 minutes. Incremental builds are fast. Accepted tradeoff for Rust's guarantees. |

---

## References

- [1] Full architecture specification: [`docs/current-arch/ARCHITECTURE.md`](../current-arch/ARCHITECTURE.md)
- [2] Audit notes (risk assessment): [`docs/current-arch/AUDIT-NOTES.md`](../current-arch/AUDIT-NOTES.md)
- [3] Security model: [`docs/current-arch/SECURITY.md`](../current-arch/SECURITY.md)
- [4] Wasmtime security: `https://docs.wasmtime.dev/security.html`
- [5] rustls audit: `https://github.com/rustls/rustls#audits`
