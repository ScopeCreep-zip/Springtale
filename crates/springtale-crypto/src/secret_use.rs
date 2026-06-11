//! Typed helpers for using `SecretBox<T>` values without leaking them.
//!
//! These wrappers are the recommended access path for every legitimate
//! exposure shape we ship — HTTP Bearer / API-key headers, SecretBox
//! re-wrap, AEAD/KDF key bytes, HMAC initialization, raw-pointer pinning
//! for `mlock`, and constant-time equality for tests. The convention is
//! "if you find yourself reaching for `.expose_secret()` outside this
//! module, look here first — it's almost certainly a shape we already
//! support."
//!
//! The convention is enforced at code-review time backed by the
//! `// SECURITY: <reason>` comment requirement in
//! `.claude/rules/backend/security.md`. It is intentionally NOT
//! enforced via `clippy.toml` `disallowed-methods`: doing so would force
//! every helper here to carry an internal `#[allow]` override, which is
//! precisely the source-level suppression we're trying to avoid.
//!
//! Shapes:
//! - **HTTP Bearer token** — [`bearer_header`].
//! - **Other HTTP header value** ([`x-api-key`, `xi-api-key`]) — [`header_value`].
//! - **Re-wrap into a fresh `SecretBox`** — [`clone_into_box`].
//! - **AEAD/KDF key bytes** — [`with_key32`] (closure-scoped).
//! - **Arbitrary byte slice** — [`with_bytes`].
//! - **HMAC initialization from `Secret<String>`** — [`with_hmac_key`].
//! - **`&str` consumer** — [`with_str`].
//! - **Raw pointer for `mlock`/`munlock`/`madvise`** — [`with_key32_ptr`].
//! - **Constant-time equality (tests)** — [`secret_eq_key32`], [`secret_eq_str`].

use secrecy::{ExposeSecret, SecretBox, SecretString};

/// Build an `Authorization: Bearer <token>` header value from a wrapped
/// secret. The intermediate exposure is bounded to this function's
/// stack frame.
// SECURITY: encapsulated single-frame exposure for HTTP Bearer auth.
pub fn bearer_header(secret: &SecretBox<String>) -> String {
    format!("Bearer {}", secret.expose_secret())
}

/// Clone the secret string out as a plain `String`. Use for HTTP headers
/// other than Bearer (e.g. `x-api-key`, `xi-api-key`) where the framework
/// wants the value directly.
// SECURITY: encapsulated single-frame exposure for API-key style HTTP headers.
pub fn header_value(secret: &SecretBox<String>) -> String {
    secret.expose_secret().clone()
}

/// Build an `Authorization: Basic <base64(user:password)>` header value.
/// The password exposure is bounded to this function's stack frame; only
/// the base64 of `user:password` leaves it.
// SECURITY: encapsulated single-frame exposure for HTTP Basic auth.
pub fn basic_auth_header(username: &str, password: &SecretBox<String>) -> String {
    use base64::Engine;
    let raw = format!("{username}:{}", password.expose_secret());
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
    format!("Basic {encoded}")
}

/// Re-wrap a secret string into a fresh `SecretBox<String>`. Used when
/// threading a config-owned secret into a long-lived struct that needs
/// independent ownership.
// SECURITY: rewrapping into a fresh SecretBox preserves the Secret<T> envelope.
pub fn clone_into_box(secret: &SecretBox<String>) -> SecretBox<String> {
    SecretBox::new(Box::new(secret.expose_secret().clone()))
}

/// Pass a 32-byte key to a closure. The closure runs with the bytes
/// borrowed; on return, the secret stays wrapped and zeroize-on-drop
/// keeps its invariant.
///
/// Use for AEAD construction (`XChaCha20Poly1305::new_from_slice`), HKDF
/// derivation, and any other operation that wants raw key bytes briefly.
// SECURITY: closure-scoped exposure for AEAD/KDF/HKDF construction.
pub fn with_key32<R>(secret: &SecretBox<[u8; 32]>, f: impl FnOnce(&[u8; 32]) -> R) -> R {
    f(secret.expose_secret())
}

/// Pass an arbitrary byte slice to a closure. Use for HMAC initialization
/// and any other byte-oriented secret consumer.
// SECURITY: closure-scoped exposure for byte-oriented secret consumers.
pub fn with_bytes<R>(secret: &SecretBox<Vec<u8>>, f: impl FnOnce(&[u8]) -> R) -> R {
    f(secret.expose_secret().as_slice())
}

/// Pass the UTF-8 bytes of a secret string to a closure. Named for the
/// common HMAC initialization site; equivalent to a string-flavoured
/// [`with_bytes`].
// SECURITY: closure-scoped exposure for HMAC initialization from a Secret<String>.
pub fn with_hmac_key<R>(secret: &SecretBox<String>, f: impl FnOnce(&[u8]) -> R) -> R {
    f(secret.expose_secret().as_bytes())
}

/// Pass the secret string as `&str` to a closure. Use for callers that
/// want a string slice — URL-encoding, format!-ing into a multi-field
/// body, comparison helpers — where the lifetime is naturally scoped
/// to the closure body.
// SECURITY: closure-scoped &str exposure for callers that consume the secret as a string slice.
pub fn with_str<R>(secret: &SecretBox<String>, f: impl FnOnce(&str) -> R) -> R {
    f(secret.expose_secret().as_str())
}

/// `with_str` for [`SecretString`] (= `SecretBox<str>`). Distinct type
/// from [`SecretBox<String>`] in the secrecy crate, so this is a
/// separate entry point.
// SECURITY: closure-scoped &str exposure for SecretString consumers.
pub fn with_secret_string<R>(secret: &SecretString, f: impl FnOnce(&str) -> R) -> R {
    f(secret.expose_secret())
}

/// HMAC initialization from a [`SecretString`]. Mirrors
/// [`with_hmac_key`] but takes `&SecretBox<str>`.
// SECURITY: closure-scoped exposure for HMAC initialization from a SecretString.
pub fn with_secret_string_bytes<R>(secret: &SecretString, f: impl FnOnce(&[u8]) -> R) -> R {
    f(secret.expose_secret().as_bytes())
}

/// Pass a raw pointer + length to a 32-byte secret to a closure. Used
/// only by `mlock` / `munlock` / `madvise(MADV_DONTDUMP)` which require
/// stable raw pointers into the secret's memory page. The pointer is
/// valid for the duration of the closure body.
// SECURITY: closure-scoped raw-pointer exposure for memsec mlock/munlock/madvise.
pub fn with_key32_ptr<R>(secret: &SecretBox<[u8; 32]>, f: impl FnOnce(*const u8, usize) -> R) -> R {
    let bytes = secret.expose_secret();
    f(bytes.as_ptr(), bytes.len())
}

/// Constant-time equality for two 32-byte secret keys. Use in tests
/// asserting KDF determinism instead of comparing the raw bytes.
///
/// Constant-time is overkill for test code, but it keeps the API uniform
/// with the production paths and means the helper has only one shape.
// SECURITY: constant-time comparison wrapper consolidates test-fixture exposure.
pub fn secret_eq_key32(a: &SecretBox<[u8; 32]>, b: &SecretBox<[u8; 32]>) -> bool {
    use subtle::ConstantTimeEq;
    a.expose_secret().ct_eq(b.expose_secret()).into()
}

/// Constant-time equality for a `SecretBox<String>` against a plain
/// `&str` reference. Used in test fixtures that verify deserialization
/// reproduced an expected secret without exposing the raw value to the
/// assertion macro.
// SECURITY: constant-time comparison wrapper consolidates test-fixture exposure.
pub fn secret_eq_str(secret: &SecretBox<String>, expected: &str) -> bool {
    use subtle::ConstantTimeEq;
    secret
        .expose_secret()
        .as_bytes()
        .ct_eq(expected.as_bytes())
        .into()
}
