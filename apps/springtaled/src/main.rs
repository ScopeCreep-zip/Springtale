use springtaled::config;
use springtaled::runtime;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Initialize tracing (structured logging)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Load configuration
    let config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to load configuration: {e}");
            std::process::exit(1);
        }
    };

    // Boot the daemon
    if let Err(e) = runtime::boot(config).await {
        tracing::error!(error = %e, "springtaled failed");
        std::process::exit(1);
    }
}
