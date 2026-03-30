use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::TransportError;
use springtale_crypto::identity::NodeId;

/// Maximum message payload size: 16 MiB.
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// A message flowing through the transport layer.
///
/// Contains only an ID and opaque payload. No sender info, no timestamps —
/// those are handled at the payload layer by the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier (for dedup).
    pub id: Uuid,
    /// Opaque payload bytes (already encrypted by springtale-crypto at call site).
    pub payload: Vec<u8>,
}

/// Transport abstraction.
///
/// All inter-node communication routes through this trait. Phase 1 uses
/// Unix sockets, Phase 2 uses HTTP/mTLS, Phase 3 uses Veilid P2P.
/// Application code takes `Arc<dyn Transport>` — the concrete transport
/// never escapes this module.
///
/// # Cancel Safety
///
/// `recv()` MUST be cancel-safe for use in `tokio::select!`. This means
/// dropping the future returned by `recv()` must not lose a message.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a message to a peer node.
    async fn send(&self, to: &NodeId, msg: Message) -> Result<(), TransportError>;

    /// Receive the next inbound message. Cancel-safe.
    async fn recv(&self) -> Result<(NodeId, Message), TransportError>;

    /// Return this node's identity.
    fn node_id(&self) -> &NodeId;

    /// Human-readable transport name for logging.
    fn name(&self) -> &'static str;
}
