//! Daily recipes — cron-driven routines: briefings, check-ins,
//! reminders, journaling, shutdown rituals. The shape that keeps a
//! day legible.

use serde_json::json;

use super::super::types::{
    ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        daily_summary(),
        cron_runner(),
        morning_briefing(),
        weekly_checkin(),
        evening_journal_prompt(),
        hydration_reminder(),
        monthly_bills_reminder(),
        weekly_screen_digest(),
        daily_shutdown_checklist(),
    ]
}

// ── Daily summary (existing) ───────────────────────────────────

fn daily_summary() -> Recipe {
    Recipe {
        id: "daily-summary".into(),
        name: "Daily Summary".into(),
        description: "Each morning, summarise yesterday's activity from a feed.".into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "summary".into(), "ai".into()],
        connectors_used: vec![],
        ai_required: true,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "schedule".into(),
                label: "Time of day".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "0 8 * * *".into(), label: "8:00 AM".into() },
                        SelectOption { value: "0 9 * * *".into(), label: "9:00 AM".into() },
                        SelectOption { value: "0 18 * * *".into(), label: "6:00 PM".into() },
                        SelectOption { value: "0 22 * * *".into(), label: "10:00 PM".into() },
                    ],
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("0 9 * * *")),
                hint: Some("When the summary should run.".into()),
            },
            InputField {
                id: "topic".into(),
                label: "Summary topic".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("yesterday's activity")),
                hint: Some("What the AI should focus on.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "daily-summary"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "AiComplete"
prompt = "Summarise ${topic} in 5 bullet points."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Runs an AI summary on a schedule.".into()),
        },
    }
}

// ── Cron runner (existing) ─────────────────────────────────────

fn cron_runner() -> Recipe {
    Recipe {
        id: "cron-runner".into(),
        name: "Scheduled Reminder".into(),
        description: "Run a one-line reminder on a cron schedule.".into(),
        icon_id: "alarm".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "reminder".into()],
        connectors_used: vec![],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "message".into(),
                label: "Reminder text".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("What you'll see when the reminder fires.".into()),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "0 9 * * *".into(), label: "Daily at 9 AM".into() },
                        SelectOption { value: "0 12 * * *".into(), label: "Daily at noon".into() },
                        SelectOption {
                            value: "0 17 * * 5".into(),
                            label: "Fridays at 5 PM".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("0 9 * * *")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "scheduled-reminder"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "SendMessage"
text = "${message}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Fires a reminder on the schedule you pick.".into()),
        },
    }
}

// ── Morning briefing ───────────────────────────────────────────

fn morning_briefing() -> Recipe {
    Recipe {
        id: "morning-briefing".into(),
        name: "AI Morning Briefing".into(),
        description: "Today's plan + weather + news in 3 bullets, delivered to Telegram.".into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "telegram".into(), "ai".into(), "briefing".into()],
        connectors_used: vec!["connector-telegram".into()],
        ai_required: true,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
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
                label: "Telegram chat id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "prompt".into(),
                label: "Briefing prompt".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!(
                    "Give me 3 short bullets for today: (1) what to focus on, (2) one wellness check, (3) one act-of-care for someone."
                )),
                hint: Some("The AI rewrites your day's intention every morning.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "morning-briefing"

[trigger]
type = "Cron"
expression = "0 8 * * *"

[[actions]]
type = "AiComplete"
prompt = "${prompt}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "☀️ ${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("8am: AI drafts a 3-bullet day plan and sends to Telegram.".into()),
        },
    }
}

// ── Weekly self check-in ───────────────────────────────────────

fn weekly_checkin() -> Recipe {
    Recipe {
        id: "weekly-checkin".into(),
        name: "Weekly Self Check-in".into(),
        description: "Monday 10am self-reminder to plan the week.".into(),
        icon_id: "alarm".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "telegram".into(), "weekly".into()],
        connectors_used: vec!["connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
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
                label: "Telegram chat id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "message".into(),
                label: "Reminder text".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!(
                    "Week intent: top three goals? top three care commitments? one boundary you're holding?"
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "weekly-checkin"

[trigger]
type = "Cron"
expression = "0 10 * * 1"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "${message}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Mondays 10am: Telegram nudge to plan the week.".into()),
        },
    }
}

// ── Evening journal prompt ─────────────────────────────────────

fn evening_journal_prompt() -> Recipe {
    Recipe {
        id: "evening-journal-prompt".into(),
        name: "Evening Journal Prompt".into(),
        description: "9pm gentle journaling prompt via Signal.".into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "signal".into(), "journal".into()],
        connectors_used: vec!["connector-signal".into()],
        ai_required: true,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
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
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-signal".into(),
                config: json!({ "number": "${signal_number}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "evening-journal-prompt"

[trigger]
type = "Cron"
expression = "0 21 * * *"

[[actions]]
type = "AiComplete"
prompt = "Write one short, kind, specific question I can answer to reflect on today. Vary it from previous days."

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "📓 ${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("9pm: AI-drafted journaling question delivered to Signal.".into()),
        },
    }
}

// ── Hydration reminder ────────────────────────────────────────

fn hydration_reminder() -> Recipe {
    Recipe {
        id: "hydration-reminder".into(),
        name: "Hydration Reminder".into(),
        description: "Desktop notification every 2 hours during waking time-window.".into(),
        icon_id: "alarm".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "wellness".into(), "notify".into()],
        connectors_used: vec![],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption {
                            value: "0 8-20/2 * * *".into(),
                            label: "Every 2h, 8am–8pm".into(),
                        },
                        SelectOption {
                            value: "0 9-18/3 * * *".into(),
                            label: "Every 3h, 9am–6pm".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("0 8-20/2 * * *")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "hydration-reminder"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "Notify"
title = "Water"
body = "Drink something."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Gentle desktop pings to drink water.".into()),
        },
    }
}

// ── Monthly bills reminder ────────────────────────────────────

fn monthly_bills_reminder() -> Recipe {
    Recipe {
        id: "monthly-bills-reminder".into(),
        name: "Monthly Bills Reminder".into(),
        description: "Rent / bills due in 7 days — Telegram ping on the 1st & 15th.".into(),
        icon_id: "alarm".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "telegram".into(), "money".into()],
        connectors_used: vec!["connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
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
                label: "Telegram chat id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "message".into(),
                label: "Reminder text".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!(
                    "💰 Bills check-in: rent / utilities / subscriptions due this fortnight?"
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "monthly-bills-reminder"

[trigger]
type = "Cron"
expression = "0 9 1,15 * *"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "${message}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("1st + 15th of each month at 9am: bills check-in.".into()),
        },
    }
}

// ── Weekly screen-time digest ─────────────────────────────────

fn weekly_screen_digest() -> Recipe {
    Recipe {
        id: "weekly-screen-digest".into(),
        name: "Weekly Screen-Time Digest".into(),
        description:
            "Sunday morning: reads a local usage log file, AI-summarises, Telegram digest."
                .into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "shell".into(), "telegram".into(), "weekly".into()],
        connectors_used: vec!["connector-shell".into(), "connector-telegram".into()],
        ai_required: true,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "usage_log".into(),
                label: "Usage log file".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/log/usage.txt")),
                hint: Some(
                    "Append-only text file you maintain (one line per session). The recipe is shape-agnostic — AI parses whatever's there."
                        .into(),
                ),
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
                label: "Telegram chat id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "weekly-screen-digest"

[trigger]
type = "Cron"
expression = "0 10 * * 0"

[[actions]]
type = "RunShell"
command = "tail -n 200 ${usage_log}"

[[actions]]
type = "AiComplete"
prompt = "Summarise this week's usage log in 3 short bullets — total hours, dominant categories, one observation:\n${last_shell_output}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "📊 Weekly screen digest\n${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Sunday 10am: tails your usage log, AI-summarises the week, Telegram pings you."
                    .into(),
            ),
        },
    }
}

// ── End-of-day shutdown checklist ─────────────────────────────

fn daily_shutdown_checklist() -> Recipe {
    Recipe {
        id: "daily-shutdown-checklist".into(),
        name: "End-of-Day Shutdown Checklist".into(),
        description: "6pm weekday reminder: save work, close tabs, set away, lock vault.".into(),
        icon_id: "shield".into(),
        category: RecipeCategory::Daily,
        tags: vec!["cron".into(), "notify".into(), "shutdown".into()],
        connectors_used: vec![],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption {
                            value: "0 18 * * 1-5".into(),
                            label: "Weekdays at 6pm".into(),
                        },
                        SelectOption {
                            value: "0 19 * * 1-5".into(),
                            label: "Weekdays at 7pm".into(),
                        },
                        SelectOption {
                            value: "0 17 * * 1-5".into(),
                            label: "Weekdays at 5pm".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("0 18 * * 1-5")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "daily-shutdown-checklist"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "Notify"
title = "Shutdown"
body = "Save work • close tabs • set away • lock vault."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some("Weekday evenings: gentle shutdown ritual nudge.".into()),
        },
    }
}
