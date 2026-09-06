use springtale_connector::Connector;
use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};

struct BlueskyFactory;

#[async_trait::async_trait]
impl ConnectorFactory for BlueskyFactory {
    fn name(&self) -> &'static str {
        "connector-bluesky"
    }
    fn config_key(&self) -> &'static str {
        "bluesky"
    }
    fn manifest(&self) -> ConnectorManifest {
        crate::connector::build_manifest(
            &crate::triggers::trigger_declarations(),
            &crate::actions::action_declarations(),
        )
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
                "identifier": {
                    "type": "string",
                    "description": "Bluesky handle or DID (e.g. user.bsky.social)"
                },
                "password": {
                    "type": "string",
                    "description": "App password (not your main password)",
                    "x-secret": true
                },
                "pds_base": {
                    "type": "string",
                    "description": "PDS base URL",
                    "default": "https://bsky.social"
                },
                "jetstream_url": {
                    "type": "string",
                    "description": "Jetstream WebSocket URL for firehose",
                    "default": "wss://jetstream2.us-west.bsky.network/subscribe"
                }
            },
            "required": ["identifier", "password"]
        }))
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::BlueskyConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let client = crate::client::AtProtoClient::new(&config)
            .await
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(crate::BlueskyConnector::new(
            client,
            config.jetstream_url,
        )))
    }
}

inventory::submit!(FactoryEntry {
    factory: &BlueskyFactory,
});
