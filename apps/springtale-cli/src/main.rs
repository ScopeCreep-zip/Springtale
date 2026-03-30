mod cli;
mod commands;
mod output;

use anyhow::Result;
use clap::Parser;

use springtale_store::backend::sqlite::SqliteBackend;

use cli::{Cli, Command, ServerAction};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => {
            commands::init::run().await?;
        }
        Command::Server { action } => match action {
            ServerAction::Start => {
                commands::server::run().await?;
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
            .merge(figment::providers::Env::prefixed("SPRINGTALE_").map(|key| {
                key.as_str().replace("__", ".").into()
            }));

        #[derive(serde::Deserialize, Default)]
        struct PartialConfig {
            #[serde(default)]
            store: StoreSection,
        }
        #[derive(serde::Deserialize, Default)]
        struct StoreSection {
            #[serde(default = "default_db_path")]
            path: std::path::PathBuf,
        }

        let config: PartialConfig = figment.extract().unwrap_or_default();
        config.store.path
    } else {
        default_db_path()
    };

    SqliteBackend::open(&store_path)
        .map_err(|e| anyhow::anyhow!("failed to open store at {}: {e}", store_path.display()))
}

fn default_db_path() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join(".local/share"))
        })
        .map(|base| base.join("springtale/springtale.db"))
        .unwrap_or_else(|| std::path::PathBuf::from(".springtale/springtale.db"))
}
