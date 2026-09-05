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

            let name = springtale_runtime::operations::connectors::install_connector_to_store(
                store, manifest,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Installed connector: {name}");
        }
        ConnectorAction::Sign { path } => {
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                anyhow::anyhow!("failed to read manifest at {}: {e}", path.display())
            })?;
            let mut manifest: springtale_connector::ConnectorManifest =
                toml::from_str(&contents)
                    .map_err(|e| anyhow::anyhow!("failed to parse manifest TOML: {e}"))?;
            springtale_connector::manifest::verify::verify_manifest(&manifest)
                .map_err(|e| anyhow::anyhow!("manifest invalid: {e}"))?;

            let keypair = crate::commands::author::load_local_identity()?;
            let signature = springtale_connector::manifest::sign_manifest(&mut manifest, &keypair)
                .map_err(|e| anyhow::anyhow!("failed to sign manifest: {e}"))?;

            let signed = toml::to_string_pretty(&manifest)
                .map_err(|e| anyhow::anyhow!("failed to serialize signed manifest: {e}"))?;
            std::fs::write(&path, signed).map_err(|e| {
                anyhow::anyhow!("failed to write manifest at {}: {e}", path.display())
            })?;

            let pubkey_hex = hex::encode(keypair.verifying_key().to_bytes());
            println!("Signed {}", path.display());
            println!("  author:    {}", manifest.author);
            println!("  pubkey:    {pubkey_hex}");
            println!("  signature: {signature}");
            println!(
                "  Install verifies against `trusted-author:{}` — register it with `springtale author add {} --self`.",
                manifest.author, manifest.author
            );
        }
    }
    Ok(())
}
