mod cli;
mod commands;
mod output;

use anyhow::Result;
use clap::Parser;

use springtale_store::backend::sqlite::SqliteBackend;

use cli::{Cli, Command, CryptoAction, ServerAction, TravelAction, VaultAction};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => {
            commands::init::run().await?;
        }
        Command::New { template, dir } => {
            commands::new::run(&template, &dir)?;
        }
        Command::Doctor => {
            commands::doctor::run().await?;
        }
        Command::Fix { error_id } => {
            commands::fix::run(&error_id).await?;
        }
        Command::Trace { connector, rule } => {
            commands::trace::run(connector.as_deref(), rule.as_deref()).await?;
        }
        Command::Server { action } => match action {
            ServerAction::Start => {
                commands::server::run().await?;
            }
        },
        Command::Panic => {
            let store = open_store()?;
            commands::panic::run(&store).await?;
        }
        Command::Travel { action } => {
            let vault_path = springtale_store::paths::default_vault_path();
            let db_path = springtale_store::paths::default_db_path();
            let config_path = std::path::PathBuf::from("springtale.toml");
            match action {
                TravelAction::Prepare { backup_to } => {
                    let store = open_store()?;
                    commands::travel::prepare(
                        &backup_to,
                        &vault_path,
                        &db_path,
                        &config_path,
                        &store,
                    )?;
                }
                TravelAction::Restore { from } => {
                    commands::travel::restore(&from, &vault_path, &db_path, &config_path)?;
                }
            }
        }
        Command::Vault { action } => match action {
            VaultAction::DuressSetup => {
                let vault_path = springtale_store::paths::default_vault_path();
                commands::vault::duress_setup(&vault_path)?;
            }
        },
        Command::Crypto { action } => match action {
            CryptoAction::RotateVaultKey => {
                commands::crypto::rotate_vault_key()?;
            }
        },
        // Commands that need the store
        command => {
            let store = open_store()?;
            match command {
                Command::Connector { action } => {
                    commands::connector::run(action, &store, cli.json).await?;
                }
                Command::Rule { action } => {
                    commands::rule::run(action, &store, cli.json).await?;
                }
                Command::Events { limit, connector } => {
                    commands::events::run(&store, limit, connector, cli.json).await?;
                }
                Command::Memory { action } => {
                    commands::memory::run(action, &store).await?;
                }
                Command::Data { action } => {
                    commands::data::run(action, &store).await?;
                }
                Command::Agent { action } => {
                    commands::agent::run(action, &store).await?;
                }
                _ => unreachable!(),
            }
        }
    }

    Ok(())
}

/// Open the SQLite store from the default or configured path.
fn open_store() -> Result<SqliteBackend> {
    // Try loading config to get the store path
    let config_path = "springtale.toml";
    let store_path = if std::path::Path::new(config_path).exists() {
        // Parse just the store section from config
        let figment = figment::Figment::new()
            .merge(<figment::providers::Toml as figment::providers::Format>::file(config_path))
            .merge(
                figment::providers::Env::prefixed("SPRINGTALE_")
                    .map(|key| key.as_str().replace("__", ".").into()),
            );

        #[derive(serde::Deserialize, Default)]
        struct PartialConfig {
            #[serde(default)]
            store: StoreSection,
        }
        #[derive(serde::Deserialize, Default)]
        struct StoreSection {
            #[serde(default = "springtale_store::paths::default_db_path")]
            path: std::path::PathBuf,
        }

        let config: PartialConfig = figment.extract().unwrap_or_default();
        config.store.path
    } else {
        springtale_store::paths::default_db_path()
    };

    SqliteBackend::open(&store_path)
        .map_err(|e| anyhow::anyhow!("failed to open store at {}: {e}", store_path.display()))
}
