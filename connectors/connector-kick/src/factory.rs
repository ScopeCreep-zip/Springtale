use springtale_connector::error::ConnectorError;
use springtale_connector::factory::{ConnectorFactory, FactoryEntry};
use springtale_connector::Connector;

struct KickFactory;

#[async_trait::async_trait]
impl ConnectorFactory for KickFactory {
    fn name(&self) -> &'static str {
        "connector-kick"
    }
    fn config_key(&self) -> &'static str {
        "kick"
    }
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError> {
        // Kick requires an access_token alongside the config.
        // The token is provided in the same config section:
        //   [kick]
        //   access_token = "..."
        let access_token = config
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ConnectorError::Serialization(
                    "kick config requires 'access_token' field".to_owned(),
                )
            })?;
        let token = secrecy::SecretBox::new(Box::new(access_token.to_owned()));

        let config: crate::KickConfig = serde_json::from_value(config)
            .map_err(|e| ConnectorError::Serialization(e.to_string()))?;
        let connector = crate::KickConnector::new(&config, token)
            .map_err(|e| ConnectorError::ExecutionFailed(e.to_string()))?;
        Ok(Box::new(connector))
    }
}

inventory::submit!(FactoryEntry {
    factory: &KickFactory,
});
