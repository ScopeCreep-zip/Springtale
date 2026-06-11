// Springtale bans openssl. All TLS / crypto must use rustls + the
// rustcrypto stack (chacha20poly1305, ed25519-dalek, argon2, sha2, hmac).
//
// If you see this error, a dependency is trying to pull in openssl.
// Fix: switch the offending dep to `rustls-tls` features or replace it
// with a rustcrypto-backed alternative.

compile_error!(
    "openssl is banned in Springtale. All TLS must use rustls; all symmetric \
     crypto via rustcrypto. Trace with `cargo tree -i openssl` and switch to \
     rustls-tls features or rustcrypto alternatives."
);
