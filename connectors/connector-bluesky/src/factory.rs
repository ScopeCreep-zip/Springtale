use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::Connector;

struct BlueskyFactory;

#[async_trait::async_trait]
impl ConnectorFactory for BlueskyFactory {
    fn name(&self) -> &'static str {
        "connector-bluesky"
    }
    fn config_key(&self) -> &'static str {
        "bluesky"
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
        Ok(Box::new(crate::BlueskyConnector::new(client)))
    }
}

inventory::submit!(FactoryEntry {
    factory: &BlueskyFactory,
});
