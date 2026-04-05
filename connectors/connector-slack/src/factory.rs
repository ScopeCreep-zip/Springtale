use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::Connector;

struct SlackFactory;

#[async_trait::async_trait]
impl ConnectorFactory for SlackFactory {
    fn name(&self) -> &'static str {
        "connector-slack"
    }
    fn config_key(&self) -> &'static str {
        "slack"
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::SlackConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::SlackConnector::new(&config)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &SlackFactory,
});
