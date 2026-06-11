// Springtale bans openssl-sys. All TLS must use rustls.
//
// If you see this error, a dependency is transitively pulling in openssl-sys.
// Fix: switch the offending dep to `rustls-tls` features.

compile_error!(
    "openssl-sys is banned in Springtale. All TLS must use rustls. \
     Trace with `cargo tree -i openssl-sys` and disable native-tls features."
);
