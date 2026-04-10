use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, TriggerDecl};
use springtale_connector::Connector;

struct NostrFactory;

#[async_trait::async_trait]
impl ConnectorFactory for NostrFactory {
    fn name(&self) -> &'static str {
        "connector-nostr"
    }
    fn config_key(&self) -> &'static str {
        "nostr"
    }
    fn trigger_declarations(&self) -> Vec<TriggerDecl> {
        crate::triggers::trigger_declarations()
    }
    fn action_declarations(&self) -> Vec<ActionDecl> {
        crate::actions::action_declarations()
    }
    fn config_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "private_key": {
                    "type": "string",
                    "description": "Nostr private key (nsec bech32 or hex, secp256k1)",
                    "x-secret": true
                },
                "relays": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Relay URLs (at least one required)"
                },
                "dm_encryption": {
                    "type": "string",
                    "description": "DM encryption NIP",
                    "default": "nip44",
                    "enum": ["nip44", "nip04"]
                },
                "message_jitter_secs": {
                    "type": "integer",
                    "description": "Activity jitter in seconds",
                    "default": 30
                }
            },
            "required": ["private_key", "relays"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::NostrConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::NostrConnector::new(&config)
            .await
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &NostrFactory,
});
