# ADR 0004: Use `secrecy` crate's `Secret<T>` for all credentials

**Status:** Accepted
**Date:** 2026-03-28

## Context

Springtale handles a lot of secrets: vault passphrases, connector API
tokens, OAuth refresh tokens, webhook signing keys, the HMAC bearer
token for the management API, Ed25519 private keys. Each of these
needs:

- **Cannot accidentally end up in logs.** `Debug` impls for our types
  must not print the contents.
- **Cannot accidentally serialize.** A `Secret<T>` in a struct that
  derives `Serialize` should not be serialised by default.
- **Memory zeroed on drop.** When the value goes out of scope, the
  underlying bytes are overwritten before they're freed.
- **mlock'd where possible.** The OS shouldn't be able to page secret
  memory to swap (defence against forensic swap analysis).

Rust's standard `String` and `Vec<u8>` don't give us any of those
guarantees.

## Decision

Wrap every credential in `Secret<T>` from the
[`secrecy`](https://docs.rs/secrecy) crate. Concretely:

- All connector config structs declare credentials as `Secret<String>`
  (or `SecretBox<String>` for heap-allocated cases).
- Connector config structs derive **`Deserialize` only**, never
  `Serialize`. Even with `Secret<T>` redaction, a `Serialize` impl
  invites mistakes.
- `.expose_secret()` calls are individually annotated with
  `// SECURITY: expose needed for X` comments. Every annotation is a
  review point.
- `zeroize` runs on drop via `secrecy`'s `Zeroize` integration.

Enforced via `clippy::expect_used` and `clippy::unwrap_used` lints on
library crates plus manual review at the `.expose_secret()` call sites.

## Consequences

Positive:

- Compile-time prevention of secret leakage through `Debug`,
  `Display`, or `Serialize`. The type system is the enforcement, not
  developer discipline.
- Audit trail of where secrets are actually used —
  `git grep "expose_secret"` lists every site, each with a comment.
- `Zeroize` on drop means a process memory dump captures fewer
  secrets than it otherwise would.
- The `secrecy` crate is small, well-audited, and stable.

Negative:

- More boilerplate. Every connector config struct has `Secret<String>`
  fields and manual `expose_secret()` calls when constructing HTTP
  requests.
- `Secret<String>` can't be hashed or compared directly. Equality
  checks need explicit expose. Mitigated: we mostly don't compare
  secrets; when we do, it's via constant-time comparison
  (`subtle::ConstantTimeEq`).
- mlock isn't portable. We use it on Linux (`mlock(2)`) and macOS;
  Windows requires `VirtualLock` which we wire conditionally.

Locks in:

- Connector authors must use `Secret<T>` — see
  [`docs/contributing/adding-a-connector.md`](../contributing/adding-a-connector.md).
- Any new credential added anywhere in the codebase goes through this
  type, no exceptions. We've enforced this in code review at every
  one of the 14 first-party connectors.

## Alternatives considered

### Option A — `secrecy` crate (picked)

Pros and cons enumerated above.

### Option B — Roll our own newtype

Pros: total control over the API surface.
Cons: re-deriving `Drop` + `Zeroize` correctly is fiddly. Less audit
attention than the upstream crate. More code to maintain.

Why we didn't pick it: the `secrecy` crate already does exactly what
we'd write. No upside.

### Option C — `zeroize::Zeroizing` directly

Pros: smaller dependency surface (no separate `secrecy` crate).
Cons: doesn't suppress `Debug` output. Doesn't prevent `Serialize`.
We'd be reinventing those layers on top of `Zeroizing`.

Why we didn't pick it: `secrecy` builds on `Zeroize` and adds the
prevent-stupid-mistakes layer that `Zeroizing` alone doesn't have.

### Option D — Plain `String` with discipline

Pros: zero boilerplate. Familiar.
Cons: relies entirely on developer review catching every mistake.
"Discipline" is the security strategy with the worst track record in
software history.

Why we didn't pick it: the threat model is targeted attacks. We can't
afford a strategy that depends on every PR author and reviewer being
flawless every time.

## References

- `crates/springtale-crypto/src/vault/` — passphrase handling
- Every `connectors/connector-*/src/config.rs` — `Secret<String>` fields
- `.claude/rules/backend/security.md` — the rule that enforces this
- [`secrecy` docs](https://docs.rs/secrecy)
- [`zeroize` docs](https://docs.rs/zeroize)
- Related: ADR 0010 (default-deny approval gate — also about
  prevent-stupid-mistakes by default)
