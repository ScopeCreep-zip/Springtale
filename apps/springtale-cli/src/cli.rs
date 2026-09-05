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

#[derive(Subcommand, Debug)]
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
    /// Run Springtale — alias for `server start`, matches plan §16.4
    /// (`springtale init cli-runner && springtale run` in ≤60s).
    Run,
    /// Container healthcheck — probes the daemon's /health endpoint
    /// and exits 0 on 2xx, non-zero otherwise. Used by Dockerfile
    /// `HEALTHCHECK` and docker-compose because the distroless final
    /// image has no `wget` / `curl`.
    Healthcheck {
        /// Override the management API base URL.
        /// Defaults to `http://127.0.0.1:8080` (matches `springtaled` default).
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        url: String,
    },
    /// Initialize Springtale (create data directory, vault, config).
    /// After setup, optionally links a chat platform and starts the daemon.
    ///
    /// With a `<template>` argument, scaffolds that starter first then
    /// runs the vault/DB setup — matches plan §16.4's success-criterion
    /// command line: `springtale init cli-runner && springtale run`.
    Init {
        /// Optional template name — if given, equivalent to running
        /// `springtale new <template>` then the interactive init.
        template: Option<String>,
    },
    /// Create a new project from a starter template.
    New {
        /// Template name. Run `springtale new --help` for the full list, or see
        /// `docs/guide/templates.md`. 14 starters ship: telegram-bot,
        /// github-monitor, cron-runner, llm-assistant, blank-bot, cli-runner,
        /// llm-swarm, discord-bot, matrix-bot, webhook-receiver, file-watcher,
        /// research-assistant, code-review-swarm, meeting-summarizer.
        template: String,
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
    /// Bot pairing management — generate codes and revoke access.
    Bot {
        #[command(subcommand)]
        action: BotAction,
    },
    /// Trusted connector authors — the keys manifest signatures are checked against.
    Author {
        #[command(subcommand)]
        action: AuthorAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuthorAction {
    /// Register a trusted author's Ed25519 public key.
    ///
    /// With `--self`, registers this instance's own identity (created by
    /// `springtale init`) so connectors you sign with
    /// `springtale connector sign` install like anyone else's.
    Add {
        /// Author name — must match the manifest's `author` field.
        /// Defaults to `local` with `--self`.
        name: Option<String>,
        /// Hex-encoded 32-byte Ed25519 public key. Not needed with `--self`.
        pubkey: Option<String>,
        /// Use the local identity's public key from the vault.
        #[arg(long = "self")]
        use_self: bool,
    },
    /// List trusted authors.
    List,
    /// Remove a trusted author.
    Remove {
        /// Author name.
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum BotAction {
    /// Generate a pairing code for a new user. Display on terminal only — never in chat.
    PairInit,
    /// Revoke ALL paired users and invalidate ALL outstanding codes.
    /// For emergencies — no chat access needed.
    PanicUnpair,
}

#[derive(Subcommand, Debug)]
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

#[derive(Subcommand, Debug)]
pub enum VaultAction {
    /// Configure a duress passphrase (dual-region vault).
    DuressSetup,
}

#[derive(Subcommand, Debug)]
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
    /// Sign a connector manifest TOML file with the local identity.
    /// The signature is written back into the file.
    Sign {
        /// Path to the connector manifest TOML file.
        path: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
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

#[derive(Subcommand, Debug)]
pub enum ServerAction {
    /// Start springtaled inline.
    Start,
}

#[derive(Subcommand, Debug)]
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

#[derive(Subcommand, Debug)]
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
    /// Import a previously exported JSON snapshot into this store.
    Import {
        /// Path to the JSON export file produced by `springtale data export`.
        #[arg(long)]
        input: std::path::PathBuf,
    },
    /// Delete all user data (rules, events, sessions, memory) without destroying the vault.
    Purge,
}

#[derive(Subcommand, Debug)]
pub enum AgentAction {
    /// Set an agent's autonomy level (observe, suggest, act-with-approval, act-autonomously).
    SetAutonomy {
        /// Agent name.
        name: String,
        /// Autonomy level.
        level: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CryptoAction {
    /// Re-encrypt the vault with a new passphrase.
    RotateVaultKey,
}
