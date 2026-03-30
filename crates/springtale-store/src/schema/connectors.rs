use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Row type for the `connectors` table.
///
/// Stores registered connector manifests. The manifest is stored as JSON
/// so the store crate doesn't depend on springtale-connector types.
/// The application layer deserializes the JSON into `ConnectorManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorRow {
    /// Connector name (primary key).
    pub name: String,
    /// Connector version.
    pub version: String,
    /// Connector author.
    pub author: String,
    /// Human-readable description.
    pub description: String,
    /// Full connector manifest serialized as JSON.
    pub manifest_json: String,
    /// Whether this connector is enabled.
    pub enabled: bool,
    /// When the connector was installed.
    pub installed_at: DateTime<Utc>,
}
