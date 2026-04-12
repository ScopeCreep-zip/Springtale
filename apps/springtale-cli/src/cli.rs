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
    /// After setup, optionally links a chat platform and starts the daemon.
    Init,
    /// Create a new project from a starter template.
    New {
        /// Template name: telegram-bot, github-monitor, cron-runner, llm-assistant
        template: String,
        /// Output directory (default: current directory).
        #[arg(long, default_value = ".")]
        dir: std::path::PathBuf,
    },
    /// Diagnose configuration and connectivity issues.
    Doctor,
    /// Attempt to fix a Springtale error by its ID (E001-E009).
    Fix {
        /// Error ID (e.g., E001).
        error_id: String,
    },
    /// Real-time execution trace — watch rule triggers, action dispatches, sentinel verdicts.
    Trace {
        /// Filter by connector name.
        #[arg(long)]
        connector: Option<String>,
        /// Filter by rule name.
        #[arg(long)]
        rule: Option<String>,
    },
    /// Emergency data destruction — overwrites vault + database with random bytes.
    /// NO confirmation prompt. This is for emergencies (IPV, device seizure).
    Panic,
    /// Travel mode — encrypted backup + local wipe, or restore from backup.
    Travel {
        #[command(subcommand)]
        action: TravelAction,
    },
    /// Vault management — duress passphrase setup.
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Bot memory inspection and maintenance.
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Data export and purge.
    Data {
        #[command(subcommand)]
        action: DataAction,
    },
    /// Agent configuration.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Cryptographic operations.
    Crypto {
        #[command(subcommand)]
        action: CryptoAction,
    },
}

#[derive(Subcommand)]
pub enum TravelAction {
    /// Export encrypted backup and wipe local data.
    Prepare {
        /// Path to save the encrypted backup file.
        #[arg(long)]
        backup_to: std::path::PathBuf,
    },
    /// Restore data from an encrypted backup.
    Restore {
        /// Path to the encrypted backup file.
        #[arg(long)]
        from: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
pub enum VaultAction {
    /// Configure a duress passphrase (dual-region vault).
    DuressSetup,
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
    /// Delete a rule by ID.
    Delete {
        /// Rule ID.
        id: String,
    },
    /// Update a rule from a JSON or TOML file.
    Update {
        /// Rule ID to update.
        id: String,
        /// Path to the updated rule definition file.
        file: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
pub enum ServerAction {
    /// Start springtaled inline.
    Start,
}

#[derive(Subcommand)]
pub enum MemoryAction {
    /// Inspect bot memory — list sessions and entry counts.
    Audit,
    /// Force memory compaction — delete oldest entries beyond limit.
    Compact {
        /// Maximum entries per session (default: 100).
        #[arg(long, default_value = "100")]
        max_entries: usize,
    },
}

#[derive(Subcommand)]
pub enum DataAction {
    /// Export all user data to a JSON file.
    Export {
        /// Output file path (default: stdout).
        #[arg(long)]
        output: Option<std::path::PathBuf>,
        /// Encrypt the export file.
        #[arg(long)]
        encrypt: bool,
    },
    /// Delete all user data (rules, events, sessions, memory) without destroying the vault.
    Purge,
}

#[derive(Subcommand)]
pub enum AgentAction {
    /// Set an agent's autonomy level (observe, suggest, act-with-approval, act-autonomously).
    SetAutonomy {
        /// Agent name.
        name: String,
        /// Autonomy level.
        level: String,
    },
}

#[derive(Subcommand)]
pub enum CryptoAction {
    /// Re-encrypt the vault with a new passphrase.
    RotateVaultKey,
}
