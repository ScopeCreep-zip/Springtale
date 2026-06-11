use springtaled::config;
use springtaled::runtime;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Install rustls CryptoProvider before any TLS usage.
    //
    // Our dep tree has both `ring` and `aws-lc-rs` active (ring from our
    // workspace Cargo.toml, aws-lc-rs from axum-server's tls-rustls feature).
    // Without an explicit install, rustls's auto-resolution panics when it
    // finds both.
    //
    // We install the post-quantum-preferring provider from `rustls-post-
    // quantum`, which is the same `ring` backend with `X25519MLKEM768` added
    // to `kx_groups` ahead of pure X25519. Hybrid handshake is negotiated
    // with peers that support it; classical X25519 is the fallback. Required
    // for NIST IR 8547 alignment (X25519 disallowed 2035) and our
    // `docs/security/CRYPTO-INVENTORY.md` 2026 Q4 milestone.
    springtale_transport::crypto_provider::install_default_pq();

    // Initialize tracing (structured logging)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Load configuration
    let loaded = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to load configuration: {e}");
            std::process::exit(1);
        }
    };

    // Boot the daemon
    if let Err(e) = runtime::boot(loaded.config, loaded.connector_configs).await {
        tracing::error!(error = %e, "springtaled failed");
        std::process::exit(1);
    }
}
