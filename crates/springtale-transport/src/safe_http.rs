//! HTTP client factory enforcing Springtale's transport policy.
//!
//! Every `reqwest::Client` in the workspace constructed via this module
//! carries Springtale's baseline:
//!
//! - rustls TLS only (`native-tls` and `openssl` are compile-time stubs in
//!   `vendor/*-stub/` per workspace `[patch.crates-io]`).
//! - Post-quantum-preferring KEX (X25519MLKEM768 hybrid) via the
//!   process-global crypto provider installed by
//!   [`crypto_provider::install_default_pq`].
//! - 30s default request timeout.
//! - 5s default connect timeout.
//! - Up to 5 redirects, then refuse (no infinite-loop SSRF amplifier).
//!
//! Per `docs/security/CRYPTO-INVENTORY.md`, OWASP A03 (Software Supply Chain
//! Failures) and A10 (Unbounded Consumption).
//!
//! `safe_http::builder` / `safe_http::client` is the recommended (and
//! conventionally-only) construction path for `reqwest::Client` in this
//! workspace. The convention is enforced at code-review time; we do not
//! add `reqwest::Client::*` to `clippy.toml` `disallowed-methods` because
//! doing so would force this very module to carry an internal `#[allow]`
//! override — the kind of source-level suppression the lint was meant
//! to prevent.
//!
//! [`crypto_provider::install_default_pq`]: crate::crypto_provider::install_default_pq

use std::time::Duration;

use crate::error::TransportError;

/// Default per-request wall-clock timeout. Aligned with OWASP A10
/// (Unbounded Consumption) and the WASM-sandbox 30s wall clock.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Default TCP+TLS handshake timeout. Five seconds is enough for any
/// reachable peer; longer indicates a network-level problem the caller
/// should hear about quickly.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of redirects to follow before refusing. Caps the
/// blast radius of a chained-redirect SSRF.
pub const DEFAULT_REDIRECT_LIMIT: usize = 5;

/// Construct a [`reqwest::ClientBuilder`] pre-configured with Springtale's
/// transport defaults.
///
/// Callers chain their own bearer headers, mTLS identity, additional
/// timeouts, root certificates, etc. on top.
pub fn builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(DEFAULT_REQUEST_TIMEOUT)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(DEFAULT_REDIRECT_LIMIT))
}

/// Convenience: build a [`reqwest::Client`] with Springtale's defaults.
///
/// Equivalent to `safe_http::builder().build()`. Errors propagate as
/// [`TransportError::Http`] with a leading "safe_http client build:"
/// prefix so log readers can attribute the failure to this factory.
pub fn client() -> Result<reqwest::Client, TransportError> {
    builder()
        .build()
        .map_err(|e| TransportError::Http(format!("safe_http client build: {e}")))
}
