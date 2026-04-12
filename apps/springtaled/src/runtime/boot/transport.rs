use std::sync::Arc;

use anyhow::{Context, Result};

use springtale_crypto::identity::keypair::Keypair;
use springtale_transport::local::unix_socket::LocalTransport;

/// Initialize transport layer (local Unix socket or HTTP with mTLS).
pub(super) async fn init_transport(
    transport_config: &crate::config::TransportConfig,
    keypair: &Keypair,
) -> Result<Arc<dyn springtale_transport::Transport>> {
    let node_id = keypair.node_id();
    let transport: Arc<dyn springtale_transport::Transport> = match transport_config
        .transport_type
        .as_str()
    {
        "http" => {
            let http_config = transport_config.http.clone().ok_or_else(|| {
                anyhow::anyhow!("transport type is 'http' but [transport.http] config is missing")
            })?;
            tracing::info!(addr = %http_config.listen_addr, "binding HTTP transport (mTLS)");
            Arc::new(
                springtale_transport::http::HttpTransport::bind(node_id, http_config)
                    .await
                    .context("failed to bind HTTP transport")?,
            )
        }
        _ => {
            tracing::info!(path = %transport_config.socket_path.display(), "binding local transport");
            Arc::new(
                LocalTransport::bind(node_id, &transport_config.socket_path)
                    .await
                    .context("failed to bind local transport")?,
            )
        }
    };
    tracing::info!(transport = transport.name(), "transport initialized");
    Ok(transport)
}
