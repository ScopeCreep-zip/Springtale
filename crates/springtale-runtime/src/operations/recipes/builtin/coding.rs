//! Coding recipes — GitHub events, CI alerts, repo backups,
//! AI-assisted PR review. Connectors: github, plus messaging
//! targets (telegram, discord, signal, bluesky, nostr) for fanout.
//!
//! Patterns covered:
//! - PR-opened watchers (notification fanout)
//! - Push → channel ping
//! - Issue / comment → mobile ping
//! - Release announcements (multi-network)
//! - Weekly mirror backups
//! - AI review chain

use serde_json::json;

use super::super::types::{
    ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        github_pr_watcher(),
        code_review(),
        github_push_discord(),
        github_issue_telegram(),
        github_comment_mobile_ping(),
        github_release_multipost(),
        github_repo_mirror_backup(),
        opencode_task(),
    ]
}

// ── GitHub PR watcher ──────────────────────────────────────────

fn github_pr_watcher() -> Recipe {
    Recipe {
        id: "github-pr-watcher".into(),
        name: "GitHub PR Watcher".into(),
        description: "Notify when a pull request is opened on a repo.".into(),
        icon_id: "github".into(),
        category: RecipeCategory::Coding,
        tags: vec!["github".into(), "pr".into(), "notification".into()],
        connectors_used: vec!["connector-github".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub personal access token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Create at github.com/settings/tokens with repo scope.".into()),
            },
            InputField {
                id: "repo".into(),
                label: "Repository (owner/name)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. octocat/hello-world".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-github".into(),
                config: json!({
                    "access_token": "${github_token}",
                    "watched_repos": ["${repo}"]
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "github-pr-opened"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request"

[[conditions]]
type = "FieldEquals"
field = "action"
value = "opened"

[[actions]]
type = "SendMessage"
text = "New PR in ${repo}: ${trigger.title} (#${trigger.number})"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Watches a GitHub repository and posts a notification when a PR is opened.".into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Code review (AI) ───────────────────────────────────────────

fn code_review() -> Recipe {
    Recipe {
        id: "code-review".into(),
        name: "AI Code Review".into(),
        description: "AI reviews every new PR opened on a watched repository.".into(),
        icon_id: "wrench".into(),
        category: RecipeCategory::Coding,
        tags: vec!["github".into(), "ai".into(), "review".into()],
        connectors_used: vec!["connector-github".into()],
        ai_required: true,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub PAT (repo + write:discussion scope)".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Needed to post review comments.".into()),
            },
            InputField {
                id: "owner".into(),
                label: "Repository owner".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. octocat/hello-world".into()),
            },
            InputField {
                id: "repo_name".into(),
                label: "Repository name".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. octocat/hello-world".into()),
            },
            InputField {
                id: "review_style".into(),
                label: "Review style".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "concise".into(), label: "Concise".into() },
                        SelectOption { value: "detailed".into(), label: "Detailed".into() },
                        SelectOption { value: "mentor".into(), label: "Mentoring".into() },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("concise")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-github".into(),
                config: json!({
                    "access_token": "${github_token}",
                    "watched_repos": ["${owner}/${repo_name}"]
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "ai-code-review"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request"

[[conditions]]
type = "FieldEquals"
field = "action"
value = "opened"

[[actions]]
type = "AiComplete"
prompt = "Review this PR in ${review_style} style: ${trigger.title}\n${trigger.body}"

[[actions]]
type = "RunConnector"
connector = "connector-github"
action = "post_comment"

[actions.params]
owner = "${owner}"
repo = "${repo_name}"
issue_number = "${trigger.number}"
body = "${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Watches the repo; when a PR opens, the AI produces a review and posts it as a comment.".into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── GitHub push → Discord ──────────────────────────────────────

fn github_push_discord() -> Recipe {
    Recipe {
        id: "github-push-discord".into(),
        name: "GitHub Push → Discord".into(),
        description: "Posts an alert to Discord for every push to a configured branch.".into(),
        icon_id: "github".into(),
        category: RecipeCategory::Coding,
        tags: vec!["github".into(), "discord".into(), "push".into()],
        connectors_used: vec!["connector-github".into(), "connector-discord".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub PAT".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "repo".into(),
                label: "Repository (owner/name)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "branch".into(),
                label: "Branch to watch".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("main")),
                hint: Some("Push events on other branches are ignored.".into()),
            },
            InputField {
                id: "discord_bot_token".into(),
                label: "Discord bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "discord_application_id".into(),
                label: "Discord application id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "discord_channel_id".into(),
                label: "Discord channel id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-github".into(),
                    config: json!({
                        "access_token": "${github_token}",
                        "watched_repos": ["${repo}"]
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-discord".into(),
                    config: json!({
                        "bot_token": "${discord_bot_token}",
                        "application_id": "${discord_application_id}"
                    }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "github-push-discord"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "push"

[[conditions]]
type = "FieldEquals"
field = "ref"
value = "refs/heads/${branch}"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "${discord_channel_id}"
content = "[${repo}] ${trigger.pusher} pushed ${trigger.commits_count} commits to ${branch}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Real-time Discord alert for every push to the configured branch.".into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── GitHub issue → Telegram ────────────────────────────────────

fn github_issue_telegram() -> Recipe {
    Recipe {
        id: "github-issue-telegram".into(),
        name: "GitHub Issue → Telegram".into(),
        description: "New issues get pinged to your Telegram with title + URL + author.".into(),
        icon_id: "github".into(),
        category: RecipeCategory::Coding,
        tags: vec!["github".into(), "telegram".into(), "issues".into()],
        connectors_used: vec!["connector-github".into(), "connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub PAT".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "repo".into(),
                label: "Repository (owner/name)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "bot_token".into(),
                label: "Telegram bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "chat_id".into(),
                label: "Telegram chat id to ping".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-github".into(),
                    config: json!({
                        "access_token": "${github_token}",
                        "watched_repos": ["${repo}"]
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "github-issue-telegram"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "issue_opened"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "[${repo}] #${trigger.number} ${trigger.title} — by ${trigger.author}\n${trigger.url}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Telegram ping for every new issue opened on the watched repo.".into()),
            derived_inputs: vec![],
        },
    }
}

// ── GitHub comment → Signal mobile ping ────────────────────────

fn github_comment_mobile_ping() -> Recipe {
    Recipe {
        id: "github-comment-mobile-ping".into(),
        name: "GitHub Comment → Signal".into(),
        description: "Get a Signal ping when someone @-mentions you in an issue/PR comment."
            .into(),
        icon_id: "github".into(),
        category: RecipeCategory::Coding,
        tags: vec!["github".into(), "signal".into(), "mention".into()],
        connectors_used: vec!["connector-github".into(), "connector-signal".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub PAT".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "repo".into(),
                label: "Repository (owner/name)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "your_github_handle".into(),
                label: "Your GitHub handle".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. octocat — comments containing @octocat trigger the alert.".into()),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number (E.164)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "signal_recipient".into(),
                label: "Signal recipient (usually your own number)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-github".into(),
                    config: json!({
                        "access_token": "${github_token}",
                        "watched_repos": ["${repo}"]
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "github-comment-mobile-ping"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "issue_comment"

[[conditions]]
type = "Contains"
field = "body"
value = "@${your_github_handle}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "[${repo}#${trigger.issue_number}] ${trigger.author} mentioned you: ${trigger.body}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Filters issue/PR comment events for `@your-handle` and pings your Signal so you can ack from a phone."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── GitHub release → Bluesky + Nostr ───────────────────────────

fn github_release_multipost() -> Recipe {
    Recipe {
        id: "github-release-multipost".into(),
        name: "GitHub Release → Bluesky + Nostr".into(),
        description: "Announce new releases on Bluesky + Nostr in one move.".into(),
        icon_id: "github".into(),
        category: RecipeCategory::Coding,
        tags: vec![
            "github".into(),
            "release".into(),
            "bluesky".into(),
            "nostr".into(),
        ],
        connectors_used: vec![
            "connector-github".into(),
            "connector-bluesky".into(),
            "connector-nostr".into(),
        ],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub PAT".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "repo".into(),
                label: "Repository (owner/name)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "bsky_handle".into(),
                label: "Bluesky handle".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "bsky_app_password".into(),
                label: "Bluesky app password".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "nostr_secret_key".into(),
                label: "Nostr secret key".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "nostr_relays".into(),
                label: "Nostr relays".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("wss://relay.damus.io,wss://nos.lol")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-github".into(),
                    config: json!({
                        "access_token": "${github_token}",
                        "watched_repos": ["${repo}"]
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-bluesky".into(),
                    config: json!({
                        "handle": "${bsky_handle}",
                        "app_password": "${bsky_app_password}"
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-nostr".into(),
                    config: json!({
                        "secret_key": "${nostr_secret_key}",
                        "relay_urls": "${nostr_relays}"
                    }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "github-release-multipost"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "release"

[[conditions]]
type = "FieldEquals"
field = "action"
value = "published"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "🚀 ${repo} ${trigger.tag_name} is out: ${trigger.html_url}"

[[actions]]
type = "RunConnector"
connector = "connector-nostr"
action = "publish_note"

[actions.params]
content = "🚀 ${repo} ${trigger.tag_name} is out: ${trigger.html_url}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Release tag → Bluesky + Nostr announcement with link.".into()),
            derived_inputs: vec![],
        },
    }
}

// ── Weekly repo mirror backup ──────────────────────────────────

fn github_repo_mirror_backup() -> Recipe {
    Recipe {
        id: "github-repo-mirror-backup".into(),
        name: "Weekly Repo Backup".into(),
        description: "Mirror-clone watched repos to your local backup directory once a week."
            .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::Coding,
        tags: vec!["github".into(), "backup".into(), "local-first".into()],
        connectors_used: vec!["connector-shell".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "repo_url".into(),
                label: "Repository clone URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. https://github.com/octocat/hello-world.git".into()),
            },
            InputField {
                id: "repo_name".into(),
                label: "Local directory name".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. hello-world — used to name the bare clone.".into()),
            },
            InputField {
                id: "backup_dir".into(),
                label: "Backup directory".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/git-backups")),
                hint: Some("Must exist + be writable.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "github-repo-mirror-backup"

[trigger]
type = "Cron"
expression = "0 3 * * 0"

[[actions]]
type = "RunShell"
command = "git clone --mirror ${repo_url} ${backup_dir}/${repo_name}.git || git -C ${backup_dir}/${repo_name}.git remote update"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Sunday 3am: mirror-clones the repo to your local disk so a deplatforming or repo deletion doesn't lose history. It runs git as a system command, so Springtale asks you to approve it the first time it fires."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── OpenCode auto-fix tagged issues (W4 — code-change parity) ──

/// When a GitHub issue is opened, hand it to a local `opencode serve`
/// agent as a coding task. The agent edits files and runs commands on the
/// host, so `run_task` is mutating — the W2 chat-approval gate fronts it
/// before anything runs. (The connector's `run_task`/`continue_session`
/// tools are also chat-callable directly for ad-hoc tasks; this recipe is
/// the event-driven automation form.)
fn opencode_task() -> Recipe {
    Recipe {
        id: "opencode-issue-fix".into(),
        name: "Auto-Fix GitHub Issues".into(),
        description:
            "When an issue is opened, hand it to your local opencode agent as a coding task.".into(),
        icon_id: "terminal".into(),
        category: RecipeCategory::Coding,
        tags: vec![
            "code".into(),
            "opencode".into(),
            "github".into(),
            "agent".into(),
        ],
        connectors_used: vec!["connector-github".into(), "connector-opencode".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "github_token".into(),
                label: "GitHub PAT".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Repo scope — used to watch for new issues.".into()),
            },
            InputField {
                id: "repo".into(),
                label: "Repository (owner/name)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. octocat/hello-world".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-github".into(),
                    config: json!({
                        "access_token": "${github_token}",
                        "watched_repos": ["${repo}"]
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-opencode".into(),
                    config: json!({ "base_url": "http://127.0.0.1:4096" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "opencode-issue-fix"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "issues"

[[conditions]]
type = "FieldEquals"
field = "action"
value = "opened"

[[actions]]
type = "RunConnector"
connector = "connector-opencode"
action = "run_task"

[actions.params]
prompt = "Work on this GitHub issue: ${trigger.title}\n\n${trigger.body}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Issue opened → opencode run_task on the issue (approval-gated).".into()),
            derived_inputs: vec![],
        },
    }
}
