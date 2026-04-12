//! Connector schema introspection — available connectors and trigger/action schemas.

use serde::Serialize;

use crate::state::RuntimeState;

/// Info about a connector that CAN be installed (from factory registry).
///
/// Static descriptor (n8n pattern): all fields are available without a
/// configured/running connector instance. The frontend uses this to
/// show capabilities, config forms, and trigger/action pickers for
/// connectors that aren't loaded yet.
#[derive(Debug, Serialize, Clone)]
pub struct AvailableConnectorInfo {
    /// Connector name (e.g., "connector-telegram").
    pub name: String,
    /// Config key for TOML/config store (e.g., "telegram").
    pub config_key: String,
    /// Whether config is required to instantiate.
    pub requires_config: bool,
    /// Whether this connector is currently loaded in the registry.
    pub loaded: bool,
    /// JSON Schema describing the connector's config struct.
    /// Fields with `"x-secret": true` should be rendered as password inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<serde_json::Value>,
    /// Static trigger declarations — what events this connector can emit.
    pub triggers: Vec<TriggerSchemaInfo>,
    /// Static action declarations — what actions this connector can perform.
    pub actions: Vec<ActionSchemaInfo>,
}

/// Connector schema info — shared between springtaled API and desktop IPC.
#[derive(Debug, Serialize, Clone)]
pub struct ConnectorSchemaInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub triggers: Vec<TriggerSchemaInfo>,
    pub actions: Vec<ActionSchemaInfo>,
}

/// Trigger declaration info for schema introspection.
#[derive(Debug, Serialize, Clone)]
pub struct TriggerSchemaInfo {
    pub name: String,
    pub description: String,
    pub schema: Option<serde_json::Value>,
}

/// Action declaration info for schema introspection.
#[derive(Debug, Serialize, Clone)]
pub struct ActionSchemaInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

/// List ALL available connectors from the factory registry.
///
/// Returns both loaded and unloaded connectors. Unloaded ones need
/// config before they can be instantiated. The UI uses this to show
/// "add new connector" options.
pub async fn list_available_connectors(state: &RuntimeState) -> Vec<AvailableConnectorInfo> {
    use springtale_connector::factory::FactoryEntry;

    let registry = state.registry.read().await;
    let loaded_names: Vec<String> = registry
        .list()
        .into_iter()
        .map(|(n, _)| n.to_owned())
        .collect();

    inventory::iter::<FactoryEntry>
        .into_iter()
        .map(|entry| {
            let factory = entry.factory;
            AvailableConnectorInfo {
                name: factory.name().to_owned(),
                config_key: factory.config_key().to_owned(),
                requires_config: factory.requires_config(),
                loaded: loaded_names.iter().any(|n| n == factory.name()),
                config_schema: factory.config_schema(),
                triggers: factory
                    .trigger_declarations()
                    .into_iter()
                    .map(|t| TriggerSchemaInfo {
                        name: t.name,
                        description: t.description,
                        schema: t.schema,
                    })
                    .collect(),
                actions: factory
                    .action_declarations()
                    .into_iter()
                    .map(|a| ActionSchemaInfo {
                        name: a.name,
                        description: a.description,
                        input_schema: a.input_schema,
                        output_schema: a.output_schema,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Get all connector schemas with trigger/action declarations.
///
/// Reads registry manifests — read-only, no store needed.
pub async fn get_connector_schemas(state: &RuntimeState) -> Vec<ConnectorSchemaInfo> {
    let registry = state.registry.read().await;
    registry
        .list()
        .into_iter()
        .filter_map(|(name, _)| {
            let entry = registry.get(name)?;
            let manifest = entry.host.manifest();
            Some(ConnectorSchemaInfo {
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                description: manifest.description.clone(),
                triggers: entry
                    .host
                    .triggers()
                    .iter()
                    .map(|t| TriggerSchemaInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        schema: t.schema.clone(),
                    })
                    .collect(),
                actions: entry
                    .host
                    .actions()
                    .iter()
                    .map(|a| ActionSchemaInfo {
                        name: a.name.clone(),
                        description: a.description.clone(),
                        input_schema: a.input_schema.clone(),
                        output_schema: a.output_schema.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
}
