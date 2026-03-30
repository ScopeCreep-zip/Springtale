use anyhow::Result;
use tabled::{Table, Tabled};

use springtale_connector::ConnectorManifest;
use springtale_store::backend::sqlite::SqliteBackend;
use springtale_store::backend::trait_::StorageBackend;
use springtale_store::schema::connectors::ConnectorRow;

use crate::cli::ConnectorAction;
use crate::output;

/// Row type for the connector list table.
#[derive(Tabled)]
struct ConnectorTableRow {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "VERSION")]
    version: String,
    #[tabled(rename = "ENABLED")]
    enabled: bool,
}

/// Handle connector subcommands.
pub async fn run(action: ConnectorAction, store: &SqliteBackend, json: bool) -> Result<()> {
    match action {
        ConnectorAction::List => {
            let connectors = store.list_connectors().await?;

            if json {
                output::print_json(&connectors)?;
            } else if connectors.is_empty() {
                println!("No connectors installed.");
            } else {
                let rows: Vec<ConnectorTableRow> = connectors
                    .iter()
                    .map(|c| ConnectorTableRow {
                        name: c.name.clone(),
                        version: c.version.clone(),
                        enabled: c.enabled,
                    })
                    .collect();
                let table = Table::new(rows).to_string();
                println!("{table}");
            }
        }
        ConnectorAction::Enable { name } => {
            store.set_connector_enabled(&name, true).await?;
            println!("Enabled connector: {name}");
        }
        ConnectorAction::Disable { name } => {
            store.set_connector_enabled(&name, false).await?;
            println!("Disabled connector: {name}");
        }
        ConnectorAction::Remove { name } => {
            store.remove_connector(&name).await?;
            println!("Removed connector: {name}");
        }
        ConnectorAction::Install { path } => {
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("failed to read manifest at {}: {e}", path.display()))?;
            let manifest: ConnectorManifest = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse manifest TOML: {e}"))?;

            // Validate manifest structure
            springtale_connector::manifest::verify::verify_manifest(&manifest)
                .map_err(|e| anyhow::anyhow!("manifest validation failed: {e}"))?;

            if manifest.signature.is_some() {
                println!("  Note: manifest has signature — verification requires author key registry (Phase 2)");
            }

            let manifest_json = serde_json::to_string(&manifest)
                .map_err(|e| anyhow::anyhow!("failed to serialize manifest to JSON: {e}"))?;

            let row = ConnectorRow {
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                author: manifest.author.clone(),
                description: manifest.description.clone(),
                manifest_json,
                enabled: true,
                installed_at: chrono::Utc::now(),
            };

            store.register_connector(&row).await?;
            println!("Installed connector: {}", manifest.name);
        }
    }
    Ok(())
}
