use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::identity::node_id::NodeId;
use springtale_crypto::vault::store::Vault;

use crate::error::BotError;

/// Bot identity — an Ed25519 keypair stored in the vault.
///
/// Phase 1b: simple identity (keypair = bot ID).
/// Phase 3: HKDF derives per-community pseudonyms from this keypair.
pub struct BotId {
    node_id: NodeId,
}

impl BotId {
    /// Load or generate a bot identity keypair from the vault.
    ///
    /// Vault key: `"bot_identity"` (separate from the daemon's `"identity"` key).
    pub fn load_or_generate(vault: &mut Vault) -> Result<Self, BotError> {
        let keypair = match vault.get("bot_identity")? {
            Some(bytes) => {
                let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                    BotError::NotInitialized(
                        "bot_identity key is wrong size (expected 32 bytes)".into(),
                    )
                })?;
                Keypair::from_secret_bytes(arr)?
            }
            None => {
                let keypair = Keypair::generate()?;
                // SECURITY: expose needed to persist bot identity key material
                vault.set("bot_identity", keypair.expose_secret_bytes().to_vec())?;
                vault.save()?;
                keypair
            }
        };

        let node_id = keypair.node_id();
        Ok(Self { node_id })
    }

    /// Get the bot's node ID (public key hash).
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }
}
