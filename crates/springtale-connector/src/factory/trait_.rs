use crate::Connector;
use crate::error::ConnectorError;
use crate::manifest::types::{ActionDecl, TriggerDecl};

/// Factory for creating connector instances from configuration.
///
/// Each connector crate implements this trait on a zero-sized struct and
/// registers it via `inventory::submit!(FactoryEntry { ... })`.
/// At runtime, `springtale-runtime::init_registry()` iterates all
/// registered factories and instantiates connectors whose config
/// sections are present.
///
/// ## Descriptor vs Instance (n8n pattern)
///
/// Static discovery methods (`config_schema`, `trigger_declarations`,
/// `action_declarations`) return the same data the live connector would
/// — but without needing a configured instance. This lets the frontend
/// show capabilities for connectors that aren't loaded yet.
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

    /// JSON Schema describing the connector's config struct.
    ///
    /// Used by the frontend to render schema-driven config forms instead
    /// of a raw JSON textarea. Fields with `"x-secret": true` are rendered
    /// as masked password inputs. Returns `None` if the connector has no
    /// configurable fields.
    fn config_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Static trigger declarations — available without a live connector instance.
    ///
    /// Returns the same declarations that the constructed connector's
    /// `Connector::triggers()` would return. Delegates to the connector's
    /// standalone `trigger_declarations()` function. Used by the frontend
    /// to show available triggers for connectors that aren't loaded yet.
    fn trigger_declarations(&self) -> Vec<TriggerDecl> {
        Vec::new()
    }

    /// Static action declarations — available without a live connector instance.
    ///
    /// Returns the same declarations that the constructed connector's
    /// `Connector::actions()` would return. Delegates to the connector's
    /// standalone `action_declarations()` function. Used by the frontend
    /// to show available actions for connectors that aren't loaded yet.
    fn action_declarations(&self) -> Vec<ActionDecl> {
        Vec::new()
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
