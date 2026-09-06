use std::path::PathBuf;

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

    /// Read the vault passphrase from this file (must be chmod 600)
    /// instead of prompting. For scripts and cron.
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        conflicts_with = "passphrase_command"
    )]
    pub passphrase_file: Option<PathBuf>,

    /// Run this shell command and use its stdout as the vault passphrase
    /// instead of prompting. Lets you plug in your own keychain.
    #[arg(long, global = true, value_name = "CMD")]
    pub passphrase_command: Option<String>,
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
    /// This is the one way to start: `springtale init && springtale run`.
    /// Anything beyond the bare project comes from a recipe — browse them
    /// in the colony UI or with `springtale recipes`.
    Init,
    /// Log in to springtaled: prompts for the vault passphrase, exchanges
    /// it for a long-lived API token, and saves it (mode 0600).
    Login,
    /// Revoke the saved API token and delete it.
    Logout,
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
    /// Runtime configuration (AI adapters per level).
    Config {
        #[command(subcommand)]
        action: ConfigAction,
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
    /// Cooperation primitives — the utterance def table and its glyphs.
    Cooperation {
        #[command(subcommand)]
        action: CooperationAction,
    },
    /// Formations — composition, intent, constraints, intervention.
    ///
    /// There is deliberately no `assign` verb: you steer a formation,
    /// you do not hand work to a named member (the drum rule).
    Formation {
        #[command(subcommand)]
        action: FormationAction,
    },
    /// Recipes — the shipped automation starters.
    Recipe {
        #[command(subcommand)]
        action: RecipeAction,
    },
    /// Approval queue — the blocking gate for dangerous capabilities.
    Approval {
        #[command(subcommand)]
        action: ApprovalAction,
    },
    /// Send a chat message to the bot runtime.
    Chat {
        /// The message text.
        message: String,
        /// Session id (defaults to the local in-app session).
        #[arg(long)]
        session: Option<String>,
    },
    /// Chat sessions.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Safety configuration — disguise, panic taps.
    Safety {
        #[command(subcommand)]
        action: SafetyAction,
    },
    /// Model Context Protocol — bridge an MCP client onto the daemon.
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Colony canvas — trees, agents, formations as the UI sees them.
    Canvas {
        /// Follow live canvas updates instead of printing a snapshot.
        #[arg(long)]
        stream: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum FormationAction {
    /// List formations.
    List,
    /// Show one formation.
    Get {
        /// Formation id.
        id: String,
    },
    /// Deploy a whole team from a JSON team-spec file.
    DeployTeam {
        /// Path to the team spec (JSON).
        #[arg(long)]
        file: PathBuf,
    },
    /// Deploy a formation.
    Deploy {
        /// Formation id.
        id: String,
    },
    /// Pause a formation.
    Pause {
        /// Formation id.
        id: String,
    },
    /// Resume a paused formation.
    Resume {
        /// Formation id.
        id: String,
    },
    /// Dissolve a formation.
    Dissolve {
        /// Formation id.
        id: String,
    },
    /// Rally — intervention: spend a token to surge the formation.
    Rally {
        /// Formation id.
        id: String,
    },
    /// Show or change the formation's intent (cycles when `--set` is absent).
    Intent {
        /// Formation id.
        id: String,
        /// Set the intent explicitly (reconnoiter, execute, stabilize, surge).
        #[arg(long)]
        set: Option<String>,
    },
    /// Toggle the formation's guard constraint.
    Guard {
        /// Formation id.
        id: String,
    },
    /// Cycle the formation's autonomy constraint.
    Autonomy {
        /// Formation id.
        id: String,
    },
    /// Add a connector to the formation roster (composition).
    AddMember {
        /// Formation id.
        id: String,
        /// Connector name.
        connector: String,
    },
    /// Remove a connector from the formation roster (composition).
    RmMember {
        /// Formation id.
        id: String,
        /// Connector name.
        connector: String,
    },
    /// Show the command grid the UI renders for this formation.
    Commands {
        /// Formation id.
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum RecipeAction {
    /// List recipes.
    List {
        /// Filter by category.
        #[arg(long)]
        category: Option<String>,
    },
    /// List recipe categories.
    Categories,
    /// Show one recipe.
    Get {
        /// Recipe id.
        id: String,
    },
    /// Preview what a recipe would create.
    Preview {
        /// Recipe id.
        id: String,
        /// JSON file of `{ "values": { ... } }` inputs.
        #[arg(long)]
        inputs: Option<PathBuf>,
    },
    /// Apply a recipe.
    Apply {
        /// Recipe id.
        id: String,
        /// JSON file of `{ "values": { ... } }` inputs.
        #[arg(long)]
        inputs: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ApprovalAction {
    /// List pending approvals.
    List,
    /// Approve a pending request.
    Approve {
        /// Approval id.
        id: String,
        /// Reason recorded in the audit log.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Deny a pending request.
    Deny {
        /// Approval id.
        id: String,
        /// Reason recorded in the audit log.
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// List chat sessions.
    List,
}

#[derive(Subcommand, Debug)]
pub enum SafetyAction {
    /// Show the safety config.
    Get,
    /// Turn the disguise overlay on or off.
    Disguise {
        /// `true` to activate the disguise, `false` to clear it.
        ///
        /// A positional `bool` derives `ArgAction::SetTrue` by default,
        /// which clap rejects for a positional (it would take no value).
        /// `Set` makes it the value-taking positional the help text
        /// describes.
        #[arg(action = clap::ArgAction::Set)]
        active: bool,
    },
    /// Set how many rapid title-bar taps trigger the panic wipe.
    PanicTaps {
        /// Tap count (server-bounded to 0..=10).
        count: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum CooperationAction {
    /// Print every codepoint the utterance def table renders, one `U+XXXX`
    /// per line (the input to `scripts/build-symbol-font.sh`).
    Glyphs {
        /// Nerd Fonts `glyphnames.json`; fail if any named codepoint in
        /// `utterance/defs.rs` is missing from it or has moved.
        #[arg(long, value_name = "GLYPHNAMES_JSON")]
        check: Option<PathBuf>,
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
    /// Bot settings — persona, context window, AI tool allow-list.
    Settings {
        #[command(subcommand)]
        action: BotSettingsAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum BotSettingsAction {
    /// Show the current bot settings.
    Get,
    /// Update bot settings. Only the flags you pass are changed.
    Set {
        /// Bot display name.
        #[arg(long)]
        name: Option<String>,
        /// Response tone hint (e.g. "neutral", "warm").
        #[arg(long)]
        tone: Option<String>,
        /// Command prefix character.
        #[arg(long)]
        prefix: Option<char>,
        /// Conversation context window size.
        #[arg(long)]
        context_window: Option<usize>,
        /// AI tool allow-list entry, `connector__action` (repeatable).
        /// Passing any `--allow` REPLACES the whole list; pass none to
        /// leave it untouched, `--allow ""` to clear it.
        #[arg(long)]
        allow: Vec<String>,
    },
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
pub enum McpAction {
    /// Speak the MCP stdio transport on stdin/stdout, forwarding every
    /// message to the running daemon's `/mcp` endpoint. For editors that
    /// can only launch a subprocess. Set `SPRINGTALE_API_TOKEN` so the
    /// bridge never has to prompt for a token.
    Serve,
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
    /// Import a previously exported JSON snapshot into the running daemon.
    Import {
        /// Path to the JSON export file produced by `springtale data export`.
        #[arg(long)]
        input: std::path::PathBuf,
    },
    /// Delete all user data (rules, events, sessions, memory) without
    /// destroying the vault. Irreversible — requires `--yes`.
    Purge {
        /// Confirm the wipe. Without it, nothing is deleted.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// List every stored config key.
    List,
    /// AI adapter config — one socket per level (colony, formation, agent).
    Ai {
        #[command(subcommand)]
        action: AiConfigAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum AiConfigAction {
    /// Print the AI config a level resolves to (API key redacted).
    Get {
        /// Level: colony, formation, or agent.
        #[arg(long, value_parser = ["colony", "formation", "agent"])]
        scope: String,
        /// Formation id (scope=formation) or rule id (scope=agent).
        id: Option<String>,
    },
    /// Set the AI config for a level. The API key is read from stdin, never argv.
    Set {
        /// Level: colony, formation, or agent.
        #[arg(long, value_parser = ["colony", "formation", "agent"])]
        scope: String,
        /// Formation id (scope=formation) or rule id (scope=agent).
        id: Option<String>,
        /// Adapter type.
        #[arg(long = "type", value_parser = ["noop", "ollama", "openai", "anthropic"])]
        adapter_type: String,
        /// Model name.
        #[arg(long)]
        model: Option<String>,
        /// Provider base URL.
        #[arg(long)]
        base_url: Option<String>,
        /// Read the API key from the first line of stdin.
        #[arg(long)]
        api_key_stdin: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum AgentAction {
    /// Set an agent's autonomy level (observe, suggest, act-with-approval, act-autonomously).
    SetAutonomy {
        /// Rule name or rule id of the agent.
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// clap's own consistency check over the whole tree. `build.rs`
    /// generates completions and the man page from this same definition,
    /// so a malformed arg (e.g. a positional `bool`, which derives
    /// `SetTrue` and takes no value) breaks the build rather than
    /// panicking the first user who runs the subcommand.
    #[test]
    fn test_cli_definition_passes_clap_debug_assert() {
        Cli::command().debug_assert();
    }

    /// Walk the whole clap tree, collecting `parent/child` verb paths.
    fn verb_paths(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
        for sub in cmd.get_subcommands() {
            let path = if prefix.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{prefix} {}", sub.get_name())
            };
            out.push(path.clone());
            verb_paths(sub, &path, out);
        }
    }

    /// The drum rule: you steer a formation, you never hand work to a
    /// named member. No `assign` verb may exist anywhere in the CLI.
    #[test]
    fn test_cli_tree_has_no_assign_verb_anywhere() {
        let cmd = Cli::command();
        let mut paths = Vec::new();
        verb_paths(&cmd, "", &mut paths);
        let offenders: Vec<&String> = paths.iter().filter(|p| p.contains("assign")).collect();
        assert!(
            offenders.is_empty(),
            "assign verb(s) present, drum rule violated: {offenders:?}"
        );
    }

    /// Every formation verb belongs to composition, intent, constraints,
    /// intervention, or read-only inspection — and nothing else.
    #[test]
    fn test_formation_verbs_stay_in_the_four_groups() {
        const ALLOWED: &[&str] = &[
            // read-only inspection
            "list",
            "get",
            "commands", // composition
            "add-member",
            "rm-member",
            "deploy-team", // intent
            "intent",      // constraints
            "guard",
            "autonomy", // intervention
            "deploy",
            "pause",
            "resume",
            "dissolve",
            "rally",
        ];
        let cmd = Cli::command();
        let formation = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "formation")
            .expect("formation family exists");
        for sub in formation.get_subcommands() {
            let name = sub.get_name();
            assert!(
                ALLOWED.contains(&name),
                "formation verb `{name}` is outside the four orchestration groups"
            );
        }
    }

    /// The offline set is explicit and small. Everything else is a
    /// daemon call, so these names must keep existing.
    #[test]
    fn test_offline_commands_still_exist() {
        let cmd = Cli::command();
        let names: Vec<&str> = cmd.get_subcommands().map(|c| c.get_name()).collect();
        for offline in [
            "init",
            "vault",
            "crypto",
            "travel",
            "panic",
            "doctor",
            "healthcheck",
            "server",
        ] {
            assert!(
                names.contains(&offline),
                "offline command `{offline}` missing"
            );
        }
    }
}
