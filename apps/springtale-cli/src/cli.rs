use clap::{Parser, Subcommand};

/// Springtale — local-first, privacy-preserving automation platform.
#[derive(Parser)]
#[command(name = "springtale", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Output as JSON instead of table format.
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Manage connectors.
    Connector {
        #[command(subcommand)]
        action: ConnectorAction,
    },
    /// Manage automation rules.
    Rule {
        #[command(subcommand)]
        action: RuleAction,
    },
    /// View event log.
    Events {
        /// Maximum number of events to show.
        #[arg(long, default_value = "50")]
        limit: u32,
        /// Filter by connector name.
        #[arg(long)]
        connector: Option<String>,
    },
    /// Start the daemon inline (development mode).
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
    /// Initialize Springtale (create data directory, vault, config).
    Init,
}

#[derive(Subcommand)]
pub enum ConnectorAction {
    /// List installed connectors.
    List,
    /// Enable a connector.
    Enable {
        /// Connector name.
        name: String,
    },
    /// Disable a connector.
    Disable {
        /// Connector name.
        name: String,
    },
    /// Remove a connector.
    Remove {
        /// Connector name.
        name: String,
    },
    /// Install a connector from a TOML manifest file.
    Install {
        /// Path to the connector manifest TOML file.
        path: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
pub enum RuleAction {
    /// List all rules.
    List,
    /// Toggle a rule's enabled/disabled status.
    Toggle {
        /// Rule ID.
        id: String,
    },
    /// Add a rule from a JSON or TOML file.
    Add {
        /// Path to the rule definition file (JSON or TOML).
        file: std::path::PathBuf,
    },
    /// Run a rule against a synthetic trigger event (dry-run).
    Run {
        /// Rule ID.
        id: String,
    },
}

#[derive(Subcommand)]
pub enum ServerAction {
    /// Start springtaled inline.
    Start,
}
