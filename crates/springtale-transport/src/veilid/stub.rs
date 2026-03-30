use async_trait::async_trait;

use crate::error::TransportError;
use crate::transport::trait_::{Message, Transport};
use springtale_crypto::identity::NodeId;

/// Phase 3 stub: VeilidTransport.
///
/// This struct exists as a placeholder for the Phase 3 Veilid mesh transport.
/// All methods return errors — the real implementation wraps `rekindle_protocol::VeilidNode`
/// and implements the three-path delivery model (SMPL write, gossip, watch+inspect).
///
/// See `docs/current-arch/ARCHITECTURE.md` §6.3 and §11 for the full design.
pub struct VeilidTransport {
    _private: (), // prevent construction outside this module
}

#[async_trait]
impl Transport for VeilidTransport {
    async fn send(&self, _to: &NodeId, _msg: Message) -> Result<(), TransportError> {
        Err(TransportError::NotConnected)
    }

    async fn recv(&self) -> Result<(NodeId, Message), TransportError> {
        Err(TransportError::NotConnected)
    }

    fn node_id(&self) -> &NodeId {
        // Phase 3: this would return the Veilid node identity
        static EMPTY: NodeId = NodeId([0u8; 32]);
        &EMPTY
    }

    fn name(&self) -> &'static str {
        "veilid"
    }
}
