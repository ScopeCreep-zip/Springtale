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

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// A static starter template.
#[derive(Debug, Clone, Serialize)]
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    /// Relative files the template writes into the destination dir.
    pub files: &'static [TemplateFile],
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateFile {
    pub relative_path: &'static str,
    pub contents: &'static str,
}

/// Outcome of writing a template to disk.
#[derive(Debug, Clone, Serialize)]
pub struct WriteReport {
    pub template: &'static str,
    pub dir: PathBuf,
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

/// Write a template's files into `dir`.
///
/// Refuses to write if `dir` exists and contains any entries, or if any
/// target file already exists. Creates parent directories as needed.
pub fn write_to(name: &str, dir: &Path) -> Result<WriteReport, TemplateError> {
    let template = get(name).ok_or_else(|| TemplateError::Unknown(name.to_owned()))?;

    if dir.exists() {
        let mut entries = std::fs::read_dir(dir)?;
        if entries.next().is_some() {
            return Err(TemplateError::DestinationNotEmpty(dir.to_path_buf()));
        }
    } else {
        std::fs::create_dir_all(dir)?;
    }

    let mut created = Vec::with_capacity(template.files.len());
    for file in template.files {
        let target = dir.join(file.relative_path);
        if target.exists() {
            return Err(TemplateError::WouldOverwrite(target));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, file.contents)?;
        created.push(target);
    }

    Ok(WriteReport {
        template: template.name,
        dir: dir.to_path_buf(),
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
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn list_contains_every_known_template() {
        let names: Vec<_> = list().iter().map(|t| t.name).collect();
        assert!(names.contains(&"telegram-bot"));
        assert!(names.contains(&"github-monitor"));
        assert!(names.contains(&"cron-runner"));
        assert!(names.contains(&"llm-assistant"));
    }

    #[test]
    fn get_returns_none_for_unknown() {
        assert!(get("does-not-exist").is_none());
    }

    #[test]
    fn write_to_refuses_non_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("sentinel"), b"x").unwrap();
        let err = write_to("cron-runner", tmp.path()).unwrap_err();
        assert!(matches!(err, TemplateError::DestinationNotEmpty(_)));
    }

    #[test]
    fn write_to_creates_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("project");
        let report = write_to("telegram-bot", &dir).unwrap();
        assert_eq!(report.template, "telegram-bot");
        assert!(dir.join("springtale.toml").exists());
        assert!(dir.join("rules/welcome.toml").exists());
    }
}
