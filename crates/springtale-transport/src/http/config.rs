use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Configuration for HTTP transport with mTLS.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpTransportConfig {
    /// Address to listen on (e.g., "0.0.0.0:7373").
    pub listen_addr: String,

    /// Path to the server TLS certificate (PEM).
    pub tls_cert: PathBuf,

    /// Path to the server TLS private key (PEM).
    pub tls_key: PathBuf,

    /// Path to the CA certificate for verifying client certs (PEM).
    /// Required for mTLS.
    pub tls_ca: PathBuf,

    /// Known peers: NodeId hex → address mapping.
    /// Used for `send()` to resolve which peer to connect to.
    #[serde(default)]
    pub peers: HashMap<String, String>,
}
