use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::Connector;

struct ShellFactory;

#[async_trait::async_trait]
impl ConnectorFactory for ShellFactory {
    fn name(&self) -> &'static str {
        "connector-shell"
    }
    fn config_key(&self) -> &'static str {
        "shell"
    }
    fn requires_config(&self) -> bool {
        false
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::ShellConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        Ok(Box::new(crate::ShellConnector::new(config)))
    }
}

inventory::submit!(FactoryEntry {
    factory: &ShellFactory,
});
