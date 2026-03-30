use serde::{Deserialize, Serialize};

/// Node identity — 32-byte public key.
///
/// Veilid-compatible for Phase 3: maps directly to a Veilid `TypedKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    /// Create a NodeId from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display as hex
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl From<ed25519_dalek::VerifyingKey> for NodeId {
    fn from(key: ed25519_dalek::VerifyingKey) -> Self {
        Self(key.to_bytes())
    }
}
