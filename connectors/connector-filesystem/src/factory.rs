use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::Connector;

struct FilesystemFactory;

#[async_trait::async_trait]
impl ConnectorFactory for FilesystemFactory {
    fn name(&self) -> &'static str {
        "connector-filesystem"
    }
    fn config_key(&self) -> &'static str {
        "filesystem"
    }
    fn requires_config(&self) -> bool {
        false
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        let config: crate::FilesystemConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        Ok(Box::new(crate::FilesystemConnector::new(config)))
    }
}

inventory::submit!(FactoryEntry {
    factory: &FilesystemFactory,
});
