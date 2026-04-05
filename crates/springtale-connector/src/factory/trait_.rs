use crate::Connector;
use crate::error::ConnectorError;

/// Factory for creating connector instances from configuration.
///
/// Each connector crate implements this trait on a zero-sized struct and
/// registers it via `inventory::submit!(FactoryEntry { ... })`.
/// At runtime, `springtale-runtime::init_registry()` iterates all
/// registered factories and instantiates connectors whose config
/// sections are present.
///
/// The `create()` method is async because some connectors (e.g., Nostr, IRC)
/// perform network setup during construction.
#[async_trait::async_trait]
pub trait ConnectorFactory: Send + Sync + 'static {
    /// Canonical connector name (e.g., "connector-telegram").
    /// Must match the name in the connector's manifest.
    fn name(&self) -> &'static str;

    /// Config key in the TOML file (e.g., "telegram").
    /// Maps to `RuntimeConfig::connector_configs["telegram"]`.
    fn config_key(&self) -> &'static str;

    /// Whether this connector requires config to instantiate.
    /// Connectors like filesystem/shell can work with defaults.
    fn requires_config(&self) -> bool {
        true
    }

    /// Create a connector instance from a JSON config value.
    ///
    /// The JSON value comes from deserializing the TOML config section
    /// (e.g., `[telegram]`) as raw `serde_json::Value`. The factory
    /// deserializes it into the connector's typed config struct.
    async fn create(
        &self,
        config: serde_json::Value,
    ) -> Result<Box<dyn Connector>, ConnectorError>;
}
