use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum_server::tls_rustls::RustlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio::sync::mpsc;

use crate::error::TransportError;
use crate::transport::trait_::{MAX_MESSAGE_SIZE, Message, Transport};
use springtale_crypto::identity::NodeId;

use super::config::HttpTransportConfig;

/// Wire format for HTTP transport messages.
#[derive(serde::Serialize, serde::Deserialize)]
struct WireMessage {
    sender: [u8; 32],
    message: Message,
}

/// HTTP transport with mutual TLS (mTLS).
///
/// Phase 2a transport — LAN/VPN multi-node. Uses `axum-server` for the
/// server side with mTLS via rustls, and `reqwest` for client connections
/// with client certificate authentication.
///
/// Both server and client use rustls exclusively (no native-tls).
pub struct HttpTransport {
    node_id: NodeId,
    config: HttpTransportConfig,
    inbox: tokio::sync::Mutex<mpsc::Receiver<(NodeId, Message)>>,
    client: reqwest::Client,
    _server_handle: tokio::task::JoinHandle<()>,
}

impl HttpTransport {
    /// Bind an HttpTransport to the configured address with mTLS.
    ///
    /// The server requires client certificates signed by the configured CA.
    /// The client sends its own certificate when connecting to peers.
    pub async fn bind(
        node_id: NodeId,
        config: HttpTransportConfig,
    ) -> Result<Self, TransportError> {
        // ── Load certificates ──
        let cert_pem = std::fs::read(&config.tls_cert).map_err(|e| {
            TransportError::Tls(format!(
                "failed to read cert {}: {e}",
                config.tls_cert.display()
            ))
        })?;
        let key_pem = std::fs::read(&config.tls_key).map_err(|e| {
            TransportError::Tls(format!(
                "failed to read key {}: {e}",
                config.tls_key.display()
            ))
        })?;
        let ca_pem = std::fs::read(&config.tls_ca).map_err(|e| {
            TransportError::Tls(format!(
                "failed to read CA cert {}: {e}",
                config.tls_ca.display()
            ))
        })?;

        let certs = rustls_pemfile::certs(&mut BufReader::new(&cert_pem[..]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TransportError::Tls(format!("invalid cert PEM: {e}")))?;

        let key = rustls_pemfile::private_key(&mut BufReader::new(&key_pem[..]))
            .map_err(|e| TransportError::Tls(format!("invalid key PEM: {e}")))?
            .ok_or_else(|| TransportError::Tls("no private key found in PEM".into()))?;

        let ca_certs = rustls_pemfile::certs(&mut BufReader::new(&ca_pem[..]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TransportError::Tls(format!("invalid CA cert PEM: {e}")))?;

        // ── Build mTLS server config ──
        let mut ca_store = RootCertStore::empty();
        for cert in &ca_certs {
            ca_store
                .add(cert.clone())
                .map_err(|e| TransportError::Tls(format!("failed to add CA cert: {e}")))?;
        }

        // Client cert verifier — REQUIRES valid client cert (enforces mTLS)
        let client_verifier = WebPkiClientVerifier::builder(Arc::new(ca_store.clone()))
            .build()
            .map_err(|e| TransportError::Tls(format!("client verifier error: {e}")))?;

        let mut server_tls_config = ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certs, key)
            .map_err(|e| TransportError::Tls(format!("server TLS config error: {e}")))?;

        // Required for axum-server when using from_config()
        server_tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let rustls_config = RustlsConfig::from_config(Arc::new(server_tls_config));

        // ── Build reqwest client with client cert (for outbound mTLS) ──
        let client_identity = reqwest::Identity::from_pem(&[&cert_pem[..], &key_pem[..]].concat())
            .map_err(|e| TransportError::Tls(format!("client identity error: {e}")))?;

        let ca_cert_reqwest = reqwest::Certificate::from_pem(&ca_pem)
            .map_err(|e| TransportError::Tls(format!("CA cert error: {e}")))?;

        // Internal site: this is the only outbound-mTLS client in the
        // workspace that needs an attached client identity + custom root
        // certificate, so we layer those onto the safe_http defaults.
        let client = crate::safe_http::builder()
            .identity(client_identity)
            .add_root_certificate(ca_cert_reqwest)
            .build()
            .map_err(|e| TransportError::Http(format!("client build error: {e}")))?;

        // ── Set up inbox channel + axum router ──
        let (tx, rx) = mpsc::channel::<(NodeId, Message)>(256);

        let app_state = Arc::new(InboxSender { tx });
        let router = Router::new()
            .route("/transport/send", post(receive_message))
            .with_state(app_state);

        // ── Start axum-server with mTLS ──
        let addr: SocketAddr = config
            .listen_addr
            .parse()
            .map_err(|e| TransportError::ConnectionFailed(format!("invalid address: {e}")))?;

        let server_handle = tokio::spawn(async move {
            if let Err(e) = axum_server::bind_rustls(addr, rustls_config)
                .serve(router.into_make_service())
                .await
            {
                tracing::error!(error = %e, "HTTP transport server error");
            }
        });

        tracing::info!(
            addr = %config.listen_addr,
            node_id = %node_id,
            "HTTP transport listening (mTLS)"
        );

        Ok(Self {
            node_id,
            config,
            inbox: tokio::sync::Mutex::new(rx),
            client,
            _server_handle: server_handle,
        })
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, to: &NodeId, msg: Message) -> Result<(), TransportError> {
        let to_hex = hex::encode(to.as_bytes());
        let peer_addr =
            self.config.peers.get(&to_hex).ok_or_else(|| {
                TransportError::ConnectionFailed(format!("unknown peer: {to_hex}"))
            })?;

        let wire = WireMessage {
            sender: *self.node_id.as_bytes(),
            message: msg,
        };

        let payload =
            serde_json::to_vec(&wire).map_err(|e| TransportError::Serialization(e.to_string()))?;

        if payload.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge {
                size: payload.len(),
                limit: MAX_MESSAGE_SIZE,
            });
        }

        let url = format!("https://{peer_addr}/transport/send");
        let response = self
            .client
            .post(&url)
            .header("content-type", "application/octet-stream")
            .body(payload)
            .send()
            .await
            .map_err(|e| TransportError::Http(format!("send to {peer_addr} failed: {e}")))?;

        if !response.status().is_success() {
            return Err(TransportError::Http(format!(
                "peer returned status {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Receive the next inbound message. Cancel-safe.
    async fn recv(&self) -> Result<(NodeId, Message), TransportError> {
        let mut inbox = self.inbox.lock().await;
        inbox.recv().await.ok_or(TransportError::ChannelClosed)
    }

    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    fn name(&self) -> &'static str {
        "http"
    }
}

// ── Server internals ──────────────────────────────────────────

struct InboxSender {
    tx: mpsc::Sender<(NodeId, Message)>,
}

/// Receive a message via HTTP POST.
async fn receive_message(
    State(state): State<Arc<InboxSender>>,
    body: axum::body::Bytes,
) -> StatusCode {
    if body.len() > MAX_MESSAGE_SIZE {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }

    let wire: WireMessage = match serde_json::from_slice(&body) {
        Ok(w) => w,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let sender = NodeId::from_bytes(wire.sender);
    if state.tx.try_send((sender, wire.message)).is_err() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }

    StatusCode::OK
}
