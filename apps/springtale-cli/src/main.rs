mod cli;
mod client;
mod commands;
mod output;
mod store;

use anyhow::Result;
use clap::Parser;

use cli::{
    BotAction, Cli, Command, CooperationAction, CryptoAction, McpAction, ServerAction,
    TravelAction, VaultAction,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Install the post-quantum-preferring rustls crypto provider before any
    // TLS surface is constructed. The CLI's `server`/`run` subcommands boot
    // the daemon path which builds `rustls::ServerConfig` / `reqwest`
    // clients, both of which require a process-global provider. Calling
    // here is idempotent — `install_default_pq` returns `false` if a
    // provider is already installed.
    springtale_transport::crypto_provider::install_default_pq();

    let cli = Cli::parse();
    let pass_opts = store::PassphraseOpts {
        passphrase_file: cli.passphrase_file,
        passphrase_command: cli.passphrase_command,
    };

    match cli.command {
        Command::Init => {
            commands::init::run().await?;
        }
        Command::Login => {
            commands::login::login(cli.json).await?;
        }
        Command::Logout => {
            commands::login::logout(cli.json).await?;
        }
        Command::Doctor => {
            commands::doctor::run(&pass_opts, cli.json).await?;
        }
        Command::Fix { error_id } => {
            commands::fix::run(&error_id, &pass_opts, cli.json).await?;
        }
        Command::Trace { connector, rule } => {
            commands::trace::run(connector.as_deref(), rule.as_deref(), cli.json).await?;
        }
        Command::Server { action } => match action {
            ServerAction::Start => {
                commands::server::run(cli.json).await?;
            }
        },
        Command::Run => {
            // Plan §16.4: `springtale init cli-runner && springtale run`.
            // `run` is the plain-English name for the daemon entry point
            // so the plan's success-criterion prompt works literally.
            commands::server::run(cli.json).await?;
        }
        Command::Healthcheck { url, ready } => {
            // Used by container HEALTHCHECK — distroless has no wget/curl.
            commands::healthcheck::run(&url, ready, cli.json).await?;
        }
        Command::Panic => {
            let store = store::open_store(&pass_opts)?;
            commands::panic::run(&store, cli.json).await?;
        }
        Command::Travel { action } => {
            let vault_path = springtale_store::paths::default_vault_path();
            let db_path = springtale_store::paths::default_db_path();
            let config_path = std::path::PathBuf::from("springtale.toml");
            match action {
                TravelAction::Prepare { backup_to } => {
                    let store = store::open_store(&pass_opts)?;
                    commands::travel::prepare(
                        &backup_to,
                        &vault_path,
                        &db_path,
                        &config_path,
                        &store,
                        cli.json,
                    )?;
                }
                TravelAction::Restore { from } => {
                    commands::travel::restore(
                        &from,
                        &vault_path,
                        &db_path,
                        &config_path,
                        cli.json,
                    )?;
                }
            }
        }
        Command::Vault { action } => match action {
            VaultAction::DuressSetup => {
                let vault_path = springtale_store::paths::default_vault_path();
                commands::vault::duress_setup(&vault_path, cli.json)?;
            }
        },
        Command::Crypto { action } => match action {
            CryptoAction::RotateVaultKey => {
                commands::crypto::rotate_vault_key(cli.json)?;
            }
        },
        Command::Bot { action } => match action {
            BotAction::Status => {
                commands::bot::status(cli.json).await?;
            }
            BotAction::Formations => {
                commands::bot::formations(cli.json).await?;
            }
            BotAction::Memory => {
                commands::bot::memory(cli.json).await?;
            }
            BotAction::PairInit => {
                commands::bot::pair_init(&pass_opts, cli.json).await?;
            }
            BotAction::PanicUnpair => {
                commands::bot::panic_unpair(&pass_opts, cli.json).await?;
            }
            BotAction::Settings { action } => {
                commands::bot::settings(action, cli.json).await?;
            }
        },
        Command::Cooperation { action } => match action {
            // `glyphs` reads the compiled-in def table for the font build;
            // it must work with no daemon and no vault.
            CooperationAction::Glyphs { check } => {
                commands::cooperation::glyphs(check.as_deref(), cli.json)?;
            }
            other => {
                commands::cooperation::utterances(other, cli.json).await?;
            }
        },
        // Daemon-backed commands. The CLI is a client of springtaled:
        // these all go over the management API so an edit is visible to
        // the running daemon immediately. There is no store fallback —
        // when the daemon is down they fail with one message.
        Command::Connector { action } => {
            commands::connector::run(action, cli.json).await?;
        }
        Command::Rule { action } => {
            commands::rule::run(action, cli.json).await?;
        }
        Command::Events { limit, connector } => {
            commands::events::run(limit, connector, cli.json).await?;
        }
        Command::Memory { action } => {
            commands::memory::run(action, cli.json).await?;
        }
        Command::Data { action } => {
            commands::data::run(action, cli.json).await?;
        }
        Command::Agent { action } => {
            commands::agent::run(action, cli.json).await?;
        }
        Command::Config { action } => {
            commands::config::run(action, cli.json).await?;
        }
        Command::Formation { action } => {
            commands::formation::run(action, cli.json).await?;
        }
        Command::Recipe { action } => {
            commands::recipe::run(action, cli.json).await?;
        }
        Command::Approval { action } => {
            commands::approval::run(action, cli.json).await?;
        }
        Command::Chat { message, session } => {
            commands::chat::run(message, session, cli.json).await?;
        }
        Command::Session { action } => {
            commands::session::run(action, cli.json).await?;
        }
        Command::Safety { action } => {
            commands::safety::run(action, cli.json).await?;
        }
        Command::Mcp { action } => match action {
            McpAction::Serve => {
                commands::mcp::serve().await?;
            }
        },
        Command::Canvas {
            stream,
            connections,
        } => {
            commands::canvas::run(stream, connections, cli.json).await?;
        }
        Command::Auth { action } => {
            commands::auth::run(action, cli.json).await?;
        }
        Command::Drift { action } => {
            commands::drift::run(action, cli.json).await?;
        }
        Command::Execution { action } => {
            commands::execution::run(action, cli.json).await?;
        }
        Command::Onboarding { action } => {
            commands::onboarding::run(action, cli.json).await?;
        }
        Command::Workspace { action } => {
            commands::workspace::run(action, cli.json).await?;
        }
        Command::Send {
            connector,
            target,
            text,
        } => {
            commands::send::run(connector, target, text, cli.json).await?;
        }
        // Offline: needs the vault and the local store, not the daemon.
        Command::Author { action } => {
            let store = store::open_store(&pass_opts)?;
            commands::author::run(action, &store, cli.json).await?;
        }
    }

    Ok(())
}
