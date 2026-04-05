use springtaled::config;
use springtaled::runtime;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Install rustls CryptoProvider before any TLS usage.
    // Our dep tree has both `ring` and `aws-lc-rs` active (ring from our
    // workspace Cargo.toml, aws-lc-rs from axum-server's tls-rustls feature).
    // Without this, rustls's auto-resolution panics when it finds both.
    // We explicitly choose ring as the provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

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
