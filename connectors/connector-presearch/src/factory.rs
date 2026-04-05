use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::Connector;

struct PresearchFactory;

#[async_trait::async_trait]
impl ConnectorFactory for PresearchFactory {
    fn name(&self) -> &'static str {
        "connector-presearch"
    }
    fn config_key(&self) -> &'static str {
        "presearch"
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::PresearchConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::PresearchConnector::new(config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &PresearchFactory,
});
