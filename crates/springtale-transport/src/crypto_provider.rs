//! Global rustls crypto-provider installation.
//!
//! Springtale uses a hybrid X25519+ML-KEM-768 key exchange for TLS 1.3, per
//! [NIST IR 8547][nist-ir8547] (X25519 deprecated 2030, disallowed 2035).
//! The `rustls-post-quantum` crate wraps the `ring` backend and registers
//! the `X25519MLKEM768` group ahead of pure-X25519 in the `kx_groups` list,
//! so a peer that supports the hybrid gets it, and a peer that doesn't
//! falls back to classical X25519 without any extra hop.
//!
//! Install ONCE per process at startup, before any `rustls::ClientConfig`
//! or `rustls::ServerConfig` is built. Calling `install_default` a second
//! time is a no-op (returns `Err`).
//!
//! [nist-ir8547]: https://nvlpubs.nist.gov/nistpubs/ir/2024/NIST.IR.8547.ipd.pdf

/// Install the post-quantum-preferring crypto provider as the rustls
/// process default. Idempotent — second call is a no-op.
///
/// Returns `true` if this call installed the provider, `false` if a
/// provider was already installed (by us or by anything else in-process).
pub fn install_default_pq() -> bool {
    rustls_post_quantum::provider().install_default().is_ok()
}
