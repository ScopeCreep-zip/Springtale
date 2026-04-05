use anyhow::Result;
use tabled::{Table, Tabled};

use springtale_store::backend::sqlite::SqliteBackend;

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
            let connectors =
                springtale_runtime::operations::connectors::list_connectors_from_store(store)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;

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
            springtale_runtime::operations::connectors::enable_connector_in_store(store, &name)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Enabled connector: {name}");
        }
        ConnectorAction::Disable { name } => {
            springtale_runtime::operations::connectors::disable_connector_in_store(store, &name)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Disabled connector: {name}");
        }
        ConnectorAction::Remove { name } => {
            springtale_runtime::operations::connectors::remove_connector_from_store(store, &name)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Removed connector: {name}");
        }
        ConnectorAction::Install { path } => {
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                anyhow::anyhow!("failed to read manifest at {}: {e}", path.display())
            })?;
            let manifest: springtale_connector::ConnectorManifest = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse manifest TOML: {e}"))?;

            if manifest.signature.is_some() {
                println!(
                    "  Note: manifest has signature — verification requires author key registry (Phase 2)"
                );
            }

            let name = springtale_runtime::operations::connectors::install_connector_to_store(
                store, manifest,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Installed connector: {name}");
        }
    }
    Ok(())
}
