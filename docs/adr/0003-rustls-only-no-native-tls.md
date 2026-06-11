# ADR 0003: rustls only; native-tls banned at compile time

**Status:** Accepted
**Date:** 2026-03-28

## Context

Every outbound HTTPS call Springtale makes (connectors hitting their
upstream APIs, the `HttpTransport` between daemons, sentinel webhook
verification, OAuth flows) goes through some TLS implementation.

The Rust ecosystem offers two:

- **`rustls`** — pure-Rust TLS implementation. Audited by NCC Group.
  No OpenSSL.
- **`native-tls`** — wrapper around the OS's TLS stack (SChannel on
  Windows, Secure Transport on macOS, OpenSSL on Linux).

Most reqwest-using crates pull `native-tls` by default. If we don't
actively prevent it, we end up linking OpenSSL transitively even when
our code only uses rustls.

## Decision

`rustls` exclusively. `native-tls` is banned at the workspace level
via three independent layers:

1. **`deny.toml`** rejects `native-tls` and `openssl-sys` in
   `cargo deny check`.
2. **`Cargo.toml [patch]`** redirects `native-tls` to a vendored stub
   at `vendor/native-tls-stub/`. The stub `compile_error!()`s if
   anything actually tries to compile it.
3. **Every `reqwest` consumer in the workspace** uses
   `features = ["rustls-tls"]` and `default-features = false`.

*Update (2026-05): the stub layer was extended — `vendor/openssl-stub/`
and `vendor/openssl-sys-stub/` now apply the same `compile_error!()`
treatment to any transitive `openssl` / `openssl-sys` pull, which was
previously only caught by `cargo deny`. Clients are additionally built
through `springtale_transport::safe_http` (typed factory: rustls-only,
PQ-hybrid KEX, bounded timeouts/redirects) rather than raw
`reqwest::Client::new`, enforced by a `clippy.toml` disallowed-methods
lint.*

## Consequences

Positive:

- One TLS implementation in the binary. One CVE surface.
- No OpenSSL — no link to glibc-dependent crypto, no version-skew
  surprises across OSes.
- Reproducible builds. The TLS stack is built from source we audit;
  not "whatever OpenSSL the build host happens to have".
- `cargo deny check` enforces this in CI. If someone adds a
  native-tls-pulling crate as a dependency, the build breaks at
  PR time, not in production.
- TLS cert validation is consistent across all platforms. macOS
  doesn't quietly trust certs Linux wouldn't trust.

Negative:

- We can't easily talk to OAuth flows that demand client certs in
  the OS keychain. Mitigated: we don't currently have that
  requirement; if a connector ever needs it, the connector can
  reach into the keychain explicitly via `KeychainRead` capability.
- Some upstream crates default to native-tls and need explicit
  `default-features = false` lines in their dependency entries.
  Annoying but mechanical.
- A few historical crates don't support rustls at all. We've
  forked or replaced the ones we needed; we'd evaluate case-by-case
  for new dependencies.

Locks in:

- TLS root certificates come from `webpki-roots`, not the system
  trust store. Adding a custom CA requires editing the daemon, not
  the OS.
- Cert pinning is per-connector, not per-daemon. Pinning lives in
  the connector's client module.

## Alternatives considered

### Option A — rustls only with active enforcement (picked)

Pros and cons enumerated above.

### Option B — Allow both, let crates choose

Pros: less friction adding new dependencies.
Cons: links OpenSSL transitively. Cert validation differs across
platforms. Reproducibility goes out the window. Every CVE in OpenSSL
becomes a Springtale concern.

Why we didn't pick it: the threat model includes targeted attacks.
OpenSSL is a known-target attack surface. We'd rather pay the friction
cost than expose users to it.

### Option C — Per-platform: rustls on Linux, SChannel/Secure-Transport on Win/macOS

Pros: uses OS keychain for trust roots.
Cons: three TLS implementations in our threat model. macOS's Secure
Transport has had several silent-failure CVEs around cert chain
validation. Linux's OpenSSL is the standard one we want to avoid.
And we lose reproducibility entirely.

Why we didn't pick it: more code paths = more attack surface, and the
OS trust stores include a lot of CAs we wouldn't choose to trust.

### Option D — `rustls` for outbound, `native-tls` for inbound mTLS

Pros: some teams prefer the OS-managed cert chain for inbound.
Cons: still two TLS implementations. Cert validation differs.
Same OpenSSL surface on Linux.

Why we didn't pick it: same reasoning as Option B. The win is
illusory.

## References

- `deny.toml` — `native-tls` ban
- `Cargo.toml` `[patch]` section — vendored stub
- `vendor/native-tls-stub/src/lib.rs` — the compile_error!() stub
- [rustls audits](https://github.com/rustls/rustls#audits)
- Related: ADR 0002 (Wasmtime — same supply-chain caution)
