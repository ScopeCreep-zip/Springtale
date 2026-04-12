use std::path::{Path, PathBuf};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::error::TransportError;
use crate::transport::trait_::{MAX_MESSAGE_SIZE, Message, Transport};
use springtale_crypto::identity::NodeId;

/// Wire format: each frame is a JSON-serialized `WireMessage`.
/// Length-delimited framing handles message boundaries.
#[derive(serde::Serialize, serde::Deserialize)]
struct WireMessage {
    sender: [u8; 32],
    message: Message,
}

/// Local transport using Unix domain sockets.
///
/// Phase 1a transport — same-machine only. Uses `tokio::net::UnixListener`
/// for the server side and `UnixStream` for client connections.
///
/// Framing: `LengthDelimitedCodec` from `tokio_util` — each message is
/// prefixed with a 4-byte big-endian length. Max frame size: 16 MiB.
pub struct LocalTransport {
    node_id: NodeId,
    socket_path: PathBuf,
    /// Incoming messages from all connected peers.
    /// Wrapped in Mutex because Transport::recv takes &self, not &mut self.
    inbox: tokio::sync::Mutex<mpsc::Receiver<(NodeId, Message)>>,
    /// Handle to the accept loop task.
    _accept_handle: tokio::task::JoinHandle<()>,
}

impl LocalTransport {
    /// Bind a new LocalTransport to a Unix socket path.
    ///
    /// Creates the socket file with `0o600` permissions (Unix only).
    pub async fn bind(
        node_id: NodeId,
        socket_path: impl Into<PathBuf>,
    ) -> Result<Self, TransportError> {
        let socket_path = socket_path.into();

        // Remove stale socket file if it exists
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).map_err(|e| {
                TransportError::ConnectionFailed(format!(
                    "cannot remove stale socket {}: {e}",
                    socket_path.display()
                ))
            })?;
        }

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)?;

        #[cfg(unix)]
        set_socket_permissions(&socket_path)?;

        let (tx, rx) = mpsc::channel::<(NodeId, Message)>(256);

        let accept_handle = tokio::spawn(accept_loop(listener, tx));

        tracing::info!(
            path = %socket_path.display(),
            node_id = %node_id,
            "local transport listening"
        );

        Ok(Self {
            node_id,
            socket_path,
            inbox: tokio::sync::Mutex::new(rx),
            _accept_handle: accept_handle,
        })
    }

    /// Get the socket file path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

#[async_trait]
impl Transport for LocalTransport {
    async fn send(&self, _to: &NodeId, msg: Message) -> Result<(), TransportError> {
        // For local transport, connect to our own socket and send.
        // In a real deployment, `to` would determine which socket to connect to.
        // Phase 1a: single-machine, so we connect to our own socket path.
        let payload = serde_json::to_vec(&WireMessage {
            sender: *self.node_id.as_bytes(),
            message: msg,
        })
        .map_err(|e| TransportError::Serialization(e.to_string()))?;

        if payload.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge {
                size: payload.len(),
                limit: MAX_MESSAGE_SIZE,
            });
        }

        let stream = UnixStream::connect(&self.socket_path).await?;
        let mut framed = make_framed(stream);
        framed
            .send(Bytes::from(payload))
            .await
            .map_err(|e| TransportError::Io(std::io::Error::other(e.to_string())))?;

        Ok(())
    }

    /// Receive the next inbound message. Cancel-safe (backed by mpsc::Receiver::recv).
    async fn recv(&self) -> Result<(NodeId, Message), TransportError> {
        // Cancel-safe: tokio Mutex::lock is cancel-safe, and mpsc::Receiver::recv
        // is cancel-safe. Dropping the future does not lose a message.
        let mut inbox = self.inbox.lock().await;
        inbox.recv().await.ok_or(TransportError::ChannelClosed)
    }

    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    fn name(&self) -> &'static str {
        "local"
    }
}

impl Drop for LocalTransport {
    fn drop(&mut self) {
        // Clean up socket file
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Accept loop: listens for connections and reads messages into the inbox channel.
async fn accept_loop(listener: UnixListener, tx: mpsc::Sender<(NodeId, Message)>) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let tx = tx.clone();
                tokio::spawn(handle_connection(stream, tx));
            }
            Err(e) => {
                tracing::warn!(error = %e, "accept failed");
                // Brief backoff to avoid tight loop on persistent errors
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Handle a single connection: read all frames and forward to inbox.
async fn handle_connection(stream: UnixStream, tx: mpsc::Sender<(NodeId, Message)>) {
    let mut framed = make_framed(stream);

    while let Some(result) = framed.next().await {
        match result {
            Ok(bytes) => {
                match serde_json::from_slice::<WireMessage>(&bytes) {
                    Ok(wire) => {
                        let sender = NodeId::from_bytes(wire.sender);
                        if tx.send((sender, wire.message)).await.is_err() {
                            // Inbox receiver dropped — transport is shutting down
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "invalid message frame, dropping");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "frame read error");
                break;
            }
        }
    }
}

/// Create a length-delimited framed transport with our size limit.
fn make_framed(stream: UnixStream) -> Framed<UnixStream, LengthDelimitedCodec> {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_MESSAGE_SIZE)
        .new_framed(stream)
}

/// Set socket file permissions to 0o600.
#[cfg(unix)]
fn set_socket_permissions(path: &Path) -> Result<(), TransportError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use uuid::Uuid;

    fn temp_socket_path() -> PathBuf {
        // Unix socket paths have a length limit (108 on macOS, 104 on Linux).
        // Use a short random name to stay within limits.
        let id: u32 = rand::random();
        let dir = std::env::temp_dir().join("st_test");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{id:08x}.sock"))
    }

    fn test_node_id() -> NodeId {
        NodeId::from_bytes([1u8; 32])
    }

    #[tokio::test]
    async fn test_bind_creates_socket() {
        let path = temp_socket_path();
        let transport = LocalTransport::bind(test_node_id(), &path).await;
        assert!(transport.is_ok(), "bind failed: {:?}", transport.err());
        assert!(path.exists());
        drop(transport); // triggers Drop which removes socket
    }

    #[tokio::test]
    async fn test_send_recv_roundtrip() {
        let path = temp_socket_path();
        let transport = LocalTransport::bind(test_node_id(), &path).await.unwrap();
        // Wrap in Arc so we can use it from both send and recv
        let transport = Arc::new(transport);

        let msg = Message {
            id: Uuid::new_v4(),
            payload: b"hello transport".to_vec(),
        };
        let msg_id = msg.id;
        let target = NodeId::from_bytes([2u8; 32]);

        // Send in a background task
        let t = Arc::clone(&transport);
        let send_handle: tokio::task::JoinHandle<Result<(), TransportError>> =
            tokio::spawn(async move {
                // Small delay to ensure recv is waiting
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                t.send(&target, msg).await
            });

        // Recv should get it
        let result: Result<Result<(NodeId, Message), TransportError>, _> =
            tokio::time::timeout(std::time::Duration::from_secs(2), transport.recv()).await;

        assert!(result.is_ok(), "recv timed out");
        let (sender, received) = result.unwrap().unwrap();
        assert_eq!(sender, test_node_id());
        assert_eq!(received.id, msg_id);
        assert_eq!(received.payload, b"hello transport");

        send_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_node_id_returned() {
        let path = temp_socket_path();
        let nid = test_node_id();
        let transport = LocalTransport::bind(nid, &path).await.unwrap();
        assert_eq!(*transport.node_id(), nid);
    }

    #[tokio::test]
    async fn test_name() {
        let path = temp_socket_path();
        let transport = LocalTransport::bind(test_node_id(), &path).await.unwrap();
        assert_eq!(transport.name(), "local");
    }

    #[tokio::test]
    async fn test_drop_cleans_up_socket() {
        let path = temp_socket_path();
        {
            let _transport = LocalTransport::bind(test_node_id(), &path).await.unwrap();
            assert!(path.exists());
        }
        // After drop, socket file should be gone
        assert!(!path.exists());
    }
}
