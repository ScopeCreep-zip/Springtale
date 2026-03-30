// Springtale bans native-tls. All TLS must use rustls.
// This stub exists as a [patch.crates-io] replacement to catch any transitive
// dependency that tries to pull in native-tls/OpenSSL.
//
// If you see this error, a dependency is trying to use native-tls.
// Fix: ensure the dependency uses the `rustls-tls` feature instead.

compile_error!(
    "native-tls is banned in Springtale. All TLS must use rustls. \
     Check your dependency tree with `cargo tree -i native-tls` \
     and switch to rustls-tls features."
);
