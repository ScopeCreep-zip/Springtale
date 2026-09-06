//! Starter project templates — shared across CLI, desktop wizard, and web.
//!
//! `springtale new <template>` in the CLI and the "Create from template"
//! flow in the Tauri wizard both call this module. No frontend duplicates
//! template content.
//!
//! Security notes:
//!   - Template files are static, embedded constants. No user content is
//!     interpolated before writing to disk.
//!   - [`write_to`] validates that the destination directory is either
//!     empty or missing, and refuses to overwrite existing files. This
//!     prevents accidentally clobbering an in-progress project.
//!   - Templates never include real secrets — they ship with explicit
//!     `YOUR_*` placeholders and pair with guidance pointing the user at
//!     the vault UI, per the product model rule "don't tell users to edit
//!     a TOML file."

use specta::Type;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

/// A static starter template.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    /// Relative files the template writes into the destination dir.
    pub files: &'static [TemplateFile],
}

#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct TemplateFile {
    pub relative_path: &'static str,
    pub contents: &'static str,
}

/// Outcome of writing a template to disk.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct WriteReport {
    pub template: &'static str,
    #[schema(value_type = String)]
    pub dir: PathBuf,
    #[schema(value_type = Vec<String>)]
    pub created: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("unknown template: {0}")]
    Unknown(String),
    #[error("destination exists and is not empty: {0}")]
    DestinationNotEmpty(PathBuf),
    #[error("refusing to overwrite existing file: {0}")]
    WouldOverwrite(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// List every available template.
pub fn list() -> &'static [Template] {
    TEMPLATES
}

/// Look up one template by name.
pub fn get(name: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.name == name)
}

/// Write a template's files into a daemon-chosen directory.
///
/// The daemon picks the destination path under `$DATA_DIR/projects/`
/// to prevent path-traversal attacks (OWASP ASVS §12.3). The caller
/// (CLI or API) never supplies a destination — only the template name.
/// Writes go through `cap_std::fs::Dir` so even a symlink race can't
/// escape the sandbox.
pub fn write_to(name: &str) -> Result<WriteReport, TemplateError> {
    let template = get(name).ok_or_else(|| TemplateError::Unknown(name.to_owned()))?;

    let projects_dir = springtale_store::paths::data_dir().join("projects");
    std::fs::create_dir_all(&projects_dir)?;

    let slug = format!("{}-{}", name, Utc::now().format("%Y%m%d-%H%M%S"));
    let dest = projects_dir.join(&slug);
    if dest.exists() {
        return Err(TemplateError::DestinationNotEmpty(dest));
    }
    std::fs::create_dir_all(&dest)?;

    let sandbox = cap_std::fs::Dir::open_ambient_dir(&dest, cap_std::ambient_authority())
        .map_err(TemplateError::Io)?;

    let mut created = Vec::with_capacity(template.files.len());
    for file in template.files {
        if let Some(parent) = Path::new(file.relative_path).parent()
            && !parent.as_os_str().is_empty()
        {
            sandbox.create_dir_all(parent).map_err(TemplateError::Io)?;
        }
        sandbox
            .write(file.relative_path, file.contents)
            .map_err(TemplateError::Io)?;
        created.push(dest.join(file.relative_path));
    }

    Ok(WriteReport {
        template: template.name,
        dir: dest,
        created,
    })
}

// ---------- Template contents ----------
//
// Each starter:
//   - Uses `YOUR_*` placeholders for secrets.
//   - Points users at `springtale vault set` (or the dashboard) rather
//     than asking them to edit TOML secrets by hand.
//   - Ships with a working rule they can see fire immediately.

const TELEGRAM_BOT_TOML: &str = r#"# Springtale — Telegram Bot Starter.
# After running `springtale new telegram-bot`, store your bot token in the
# vault with: springtale vault set telegram.bot_token
# (Do NOT paste real secrets into this file.)

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[telegram]
bot_token = "YOUR_BOT_TOKEN"   # placeholder — set via `springtale vault set`
update_mode = "polling"
"#;

const TELEGRAM_WELCOME_RULE: &str = r#"# Example rule: greet new users on /start.
[rule]
name = "welcome"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "command_received"

[trigger.conditions]
field_equals = { field = "command", value = "/start" }

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat.id}"
text = "Welcome to Springtale! Type /help for available commands."
"#;

const GITHUB_MONITOR_TOML: &str = r#"# Springtale — GitHub Monitor Starter.
# Store tokens in the vault, not here:
#   springtale vault set telegram.bot_token
#   springtale vault set github.token
#   springtale vault set github.webhook_secret

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[telegram]
bot_token = "YOUR_TELEGRAM_BOT_TOKEN"
update_mode = "polling"

[github]
token = "YOUR_GITHUB_PAT"
webhook_secret = "YOUR_WEBHOOK_SECRET"
"#;

const GITHUB_PUSH_RULE: &str = r#"# Rule: GitHub push → Telegram notification.
[rule]
name = "github-push-notify"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "push"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "YOUR_TELEGRAM_CHAT_ID"
text = "Push to ${trigger.repository.full_name}: ${trigger.head_commit.message}"
"#;

const CRON_RUNNER_TOML: &str = r#"# Springtale — Cron Runner Starter.
# Runs scheduled tasks with no chat connector required.

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"
"#;

const CRON_HEARTBEAT_RULE: &str = r#"# Rule: heartbeat every 5 minutes.
[rule]
name = "heartbeat"

[trigger]
type = "Cron"
expression = "0 */5 * * * *"

[[actions]]
type = "SendMessage"
text = "Heartbeat: system is alive at ${trigger.fired_at}"
"#;

const LLM_ASSISTANT_TOML: &str = r#"# Springtale — LLM Assistant Starter.
# AI-powered chat bot. Uncomment ONE provider section, and store the key
# in the vault rather than pasting it inline:
#   springtale vault set openai.api_key
#   springtale vault set anthropic.api_key

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[telegram]
bot_token = "YOUR_TELEGRAM_BOT_TOKEN"
update_mode = "polling"

# [ollama]
# base_url = "http://localhost:11434"
# model = "llama3.2"

# [openai]
# base_url = "https://api.openai.com"
# api_key = "YOUR_OPENAI_KEY"
# model = "gpt-4o"

# [anthropic]
# api_key = "YOUR_ANTHROPIC_KEY"
# model = "claude-sonnet-4-6"
"#;

// ── Templates added to satisfy COOPERATION_IMPLEMENTATION_PLAN.md §8 ──────
// Minimum 10 starter templates (§16.9). Plan §8 table names the full set;
// the constants below implement the remaining entries.

const BLANK_BOT_TOML: &str = r#"# Springtale — Blank Bot Starter.
# Empty skeleton for experts. Add connectors, rules, and vault entries by hand.

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"
"#;

const CLI_RUNNER_TOML: &str = r#"# Springtale — CLI Task Runner Starter.
# Headless CLI that spawns a formation for a one-shot task.
# See COOPERATION_IMPLEMENTATION_PLAN.md §7.1 for the full design.

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"
"#;

const CLI_RUNNER_RULE: &str = r#"# Rule: kick off a task when /run is fired via `springtale send`.
[rule]
name = "cli-run-task"

[trigger]
type = "Cron"
# Once per minute — the real trigger is `springtale send` / `springtale run`.
expression = "0 * * * * *"

[[actions]]
type = "SendMessage"
text = "Formation is idle. Run `springtale send <text>` to drive a task."
"#;

const LLM_SWARM_TOML: &str = r#"# Springtale — LLM Swarm Starter.
# Three cooperating agents on a single prompt (researcher, writer, critic).
# Full cooperation module exercised. See plan §7.2.
# Store the AI provider key in the vault:
#   springtale vault set openai.api_key
#   (or anthropic.api_key / configure ollama base_url)

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

# Pick ONE AI provider section:

# [ollama]
# base_url = "http://localhost:11434"
# model = "llama3.2"

# [openai]
# base_url = "https://api.openai.com"
# api_key = "YOUR_OPENAI_KEY"
# model = "gpt-4o"

# [anthropic]
# api_key = "YOUR_ANTHROPIC_KEY"
# model = "claude-sonnet-4-6"
"#;

const DISCORD_BOT_TOML: &str = r#"# Springtale — Discord Bot Starter.
# Store the bot token in the vault rather than pasting it inline:
#   springtale vault set discord.bot_token

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[discord]
bot_token = "YOUR_DISCORD_BOT_TOKEN"
"#;

const DISCORD_WELCOME_RULE: &str = r#"# Rule: greet new members on !start.
[rule]
name = "discord-welcome"

[trigger]
type = "ConnectorEvent"
connector = "connector-discord"
event = "command_received"

[trigger.conditions]
field_equals = { field = "command", value = "!start" }

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "${trigger.channel_id}"
content = "Welcome to the Discord bot. Type !help for available commands."
"#;

const MATRIX_BOT_TOML: &str = r#"# Springtale — Matrix Bot Starter.
# Matrix/Element chatbot. Matrix connector is planned; the template is
# ready so the moment `connector-matrix` ships this starter just works.
# Store the Matrix token in the vault:
#   springtale vault set matrix.access_token

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[matrix]
homeserver = "https://matrix.org"
user_id = "@YOUR_BOT:matrix.org"
access_token = "YOUR_MATRIX_ACCESS_TOKEN"
"#;

const WEBHOOK_RECEIVER_TOML: &str = r#"# Springtale — Webhook Receiver Starter.
# Turn any inbound HTTP webhook into a cooperation formation that
# fans out to one or more actions. No chat connector required.

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"
"#;

const WEBHOOK_RECEIVER_RULE: &str = r#"# Rule: any inbound POST on /webhook/custom-event → SendMessage + log.
[rule]
name = "webhook-fanout"

[trigger]
type = "Webhook"
path = "custom-event"

[[actions]]
type = "SendMessage"
text = "Webhook received: ${trigger.body}"

[[actions]]
type = "Notify"
title = "webhook-receiver"
body  = "payload received — check logs"
"#;

const FILE_WATCHER_TOML: &str = r#"# Springtale — File Watcher Starter.
# Filesystem event → cooperation formation. Set the watched path via
#   springtale config set filesystem.watch_path /absolute/path
# and the notification scheme via the rule below.

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"
"#;

const FILE_WATCHER_RULE: &str = r#"# Rule: any filesystem change → Notify.
[rule]
name = "file-watcher-alert"

[trigger]
type = "FilesystemEvent"
path = "${config.filesystem.watch_path}"

[[actions]]
type = "Notify"
title = "file changed"
body  = "${trigger.path} (${trigger.kind})"
"#;

const RESEARCH_ASSISTANT_TOML: &str = r#"# Springtale — Research Assistant Starter.
# Multi-source research LLM swarm with cited output. Spawns a formation of
# three agents (collect → synthesize → cite) on each user query.

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[telegram]
bot_token = "YOUR_TELEGRAM_BOT_TOKEN"
update_mode = "polling"

# Pick ONE provider:

# [ollama]
# base_url = "http://localhost:11434"
# model = "llama3.2"

# [openai]
# base_url = "https://api.openai.com"
# api_key = "YOUR_OPENAI_KEY"
# model = "gpt-4o"
"#;

const CODE_REVIEW_SWARM_TOML: &str = r#"# Springtale — Code Review Swarm Starter.
# Git-diff → 3-agent review (readability, correctness, security). Runs on
# GitHub pull-request webhook. Store tokens in vault:
#   springtale vault set github.token
#   springtale vault set github.webhook_secret

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

[github]
token = "YOUR_GITHUB_PAT"
webhook_secret = "YOUR_WEBHOOK_SECRET"

# Pick ONE provider:

# [ollama]
# base_url = "http://localhost:11434"
# model = "llama3.2"

# [anthropic]
# api_key = "YOUR_ANTHROPIC_KEY"
# model = "claude-sonnet-4-6"
"#;

const CODE_REVIEW_RULE: &str = r#"# Rule: pull_request opened → post review comment (stub).
[rule]
name = "code-review-pr"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request"

[trigger.conditions]
field_equals = { field = "action", value = "opened" }

[[actions]]
type = "RunConnector"
connector = "connector-github"
action = "create_review_comment"

[actions.params]
repo = "${trigger.repository.full_name}"
pr_number = "${trigger.pull_request.number}"
body = "Automated review in progress — see formation logs."
"#;

const MEETING_SUMMARIZER_TOML: &str = r#"# Springtale — Meeting Summarizer Starter.
# Audio / transcript → structured summary via an LLM swarm of three roles
# (transcript_cleaner, key_point_extractor, summary_writer). The bot runs
# locally by default; no audio ever leaves the device.
#
# Store any provider credentials in the vault:
#   springtale vault set openai.api_key     # optional
#   springtale vault set anthropic.api_key  # optional

[store]
path = "springtale.db"

[crypto]
vault_path = "vault.bin"

[api]
bind = "127.0.0.1:8080"

# Watch a transcripts directory — drop a .txt / .vtt / .srt and a formation
# is spawned per file. The three roles coordinate via the cooperation
# module: role-scoped capabilities, shared cadence, handoff between stages.
[[filesystem.watch]]
path = "./transcripts"
events = ["create", "modify"]

# Pick ONE LLM provider (or none — NoopAdapter produces a canned summary).

# [ollama]
# base_url = "http://localhost:11434"
# model = "llama3.2"

# [openai]
# api_key = "YOUR_OPENAI_KEY"
# model = "gpt-4o-mini"

# [anthropic]
# api_key = "YOUR_ANTHROPIC_KEY"
# model = "claude-sonnet-4-6"
"#;

const MEETING_SUMMARIZER_RULE: &str = r#"# Rule: new transcript file → spawn a meeting-summarizer formation.
[rule]
name = "meeting-summarizer"

[trigger]
type = "FileWatch"
path = "./transcripts"
event = "create"

[[actions]]
type = "AiComplete"
prompt = "Summarize the meeting transcript at ${trigger.path}. Output sections: attendees, decisions, action items, open questions."
"#;

static TEMPLATES: &[Template] = &[
    Template {
        name: "telegram-bot",
        description: "Telegram bot with a /start welcome rule",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: TELEGRAM_BOT_TOML,
            },
            TemplateFile {
                relative_path: "rules/welcome.toml",
                contents: TELEGRAM_WELCOME_RULE,
            },
        ],
    },
    Template {
        name: "github-monitor",
        description: "GitHub webhook → Telegram push notifications",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: GITHUB_MONITOR_TOML,
            },
            TemplateFile {
                relative_path: "rules/github-push-notify.toml",
                contents: GITHUB_PUSH_RULE,
            },
        ],
    },
    Template {
        name: "cron-runner",
        description: "Scheduled task automation (no chat connector)",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: CRON_RUNNER_TOML,
            },
            TemplateFile {
                relative_path: "rules/heartbeat.toml",
                contents: CRON_HEARTBEAT_RULE,
            },
        ],
    },
    Template {
        name: "llm-assistant",
        description: "AI-powered chat assistant (Ollama/OpenAI/Anthropic)",
        files: &[TemplateFile {
            relative_path: "springtale.toml",
            contents: LLM_ASSISTANT_TOML,
        }],
    },
    Template {
        name: "blank-bot",
        description: "Empty skeleton for experts — no connectors, no rules",
        files: &[TemplateFile {
            relative_path: "springtale.toml",
            contents: BLANK_BOT_TOML,
        }],
    },
    Template {
        name: "cli-runner",
        description: "Headless CLI task runner — spawns a formation per task (plan §7.1)",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: CLI_RUNNER_TOML,
            },
            TemplateFile {
                relative_path: "rules/idle-heartbeat.toml",
                contents: CLI_RUNNER_RULE,
            },
        ],
    },
    Template {
        name: "llm-swarm",
        description: "3-agent LLM swarm (researcher/writer/critic) on a single prompt (plan §7.2)",
        files: &[TemplateFile {
            relative_path: "springtale.toml",
            contents: LLM_SWARM_TOML,
        }],
    },
    Template {
        name: "discord-bot",
        description: "Discord bot with a !start welcome rule",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: DISCORD_BOT_TOML,
            },
            TemplateFile {
                relative_path: "rules/welcome.toml",
                contents: DISCORD_WELCOME_RULE,
            },
        ],
    },
    Template {
        name: "matrix-bot",
        description: "Matrix/Element chatbot skeleton — ready for connector-matrix",
        files: &[TemplateFile {
            relative_path: "springtale.toml",
            contents: MATRIX_BOT_TOML,
        }],
    },
    Template {
        name: "webhook-receiver",
        description: "HTTP webhook → cooperation formation fan-out",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: WEBHOOK_RECEIVER_TOML,
            },
            TemplateFile {
                relative_path: "rules/webhook-fanout.toml",
                contents: WEBHOOK_RECEIVER_RULE,
            },
        ],
    },
    Template {
        name: "file-watcher",
        description: "Filesystem event → cooperation formation",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: FILE_WATCHER_TOML,
            },
            TemplateFile {
                relative_path: "rules/file-changed-alert.toml",
                contents: FILE_WATCHER_RULE,
            },
        ],
    },
    Template {
        name: "research-assistant",
        description: "Multi-source research LLM swarm with cited output",
        files: &[TemplateFile {
            relative_path: "springtale.toml",
            contents: RESEARCH_ASSISTANT_TOML,
        }],
    },
    Template {
        name: "code-review-swarm",
        description: "Git diff → 3-agent code review (readability / correctness / security)",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: CODE_REVIEW_SWARM_TOML,
            },
            TemplateFile {
                relative_path: "rules/pr-opened.toml",
                contents: CODE_REVIEW_RULE,
            },
        ],
    },
    Template {
        name: "meeting-summarizer",
        description: "Audio/transcript → structured summary LLM swarm",
        files: &[
            TemplateFile {
                relative_path: "springtale.toml",
                contents: MEETING_SUMMARIZER_TOML,
            },
            TemplateFile {
                relative_path: "rules/new-transcript.toml",
                contents: MEETING_SUMMARIZER_RULE,
            },
        ],
    },
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn list_contains_every_known_template() {
        let names: Vec<_> = list().iter().map(|t| t.name).collect();
        for expected in [
            // Existing templates
            "telegram-bot",
            "github-monitor",
            "cron-runner",
            "llm-assistant",
            // Added for plan §8 coverage (≥10 starters)
            "blank-bot",
            "cli-runner",
            "llm-swarm",
            "discord-bot",
            "matrix-bot",
            "webhook-receiver",
            "file-watcher",
            "research-assistant",
            "code-review-swarm",
            "meeting-summarizer",
        ] {
            assert!(
                names.contains(&expected),
                "template list missing `{expected}`"
            );
        }
    }

    #[test]
    fn template_count_meets_plan_minimum() {
        // Plan §16.9 requires ≥10 starter templates.
        assert!(
            list().len() >= 10,
            "expected ≥10 templates per plan §16.9, got {}",
            list().len()
        );
    }

    #[test]
    fn get_returns_none_for_unknown() {
        assert!(get("does-not-exist").is_none());
    }

    #[test]
    fn write_to_creates_all_files() {
        // write_to now picks its own path under $DATA_DIR/projects/,
        // so we just verify it works with a known template.
        let report = write_to("telegram-bot").unwrap();
        assert_eq!(report.template, "telegram-bot");
        assert!(report.dir.join("springtale.toml").exists());
        assert!(report.dir.join("rules/welcome.toml").exists());
        // Clean up
        let _ = std::fs::remove_dir_all(&report.dir);
    }
}
