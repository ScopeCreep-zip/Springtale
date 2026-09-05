//! Safety + Privacy recipes — the core Springtale mission slice.
//!
//! Grounded in published guidance from EFF, the Tor Project, Cornell
//! Tech Clinic to End Tech Abuse (CETA), Trans Army OPSEC Toolkit,
//! Activist Handbook, and IPV Tech Research. See
//! `feedback_eff_transarmy_alignment` + `feedback_universal_design_from_bottom`:
//! every recipe here is designed for the most-threatened user and
//! made universally useful — no persona labels in the UI, just
//! clearly-named tools anyone can pick up.
//!
//! Patterns:
//!   - Dead man's switch (webhook + file-watch variants)
//!   - Doxxing watch (presearch + AI risk-assess)
//!   - Panic broadcast (one URL → many channels)
//!   - Evidence archives (per-sender append-only)
//!   - Allow-list / comms-audit reminders
//!   - Periodic hygiene (Signal disappearing-timer sweep, key
//!     rotation, Tor refresh)
//!   - VPN watch
//!   - Travel-mode prep
//!   - Encrypted archive pipe
//!   - Content-moderation defuse (keyword filter, chat moderator)

use serde_json::json;

use super::super::types::{
    ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        signal_relay(),
        dead_mans_switch(),
        checkin_dead_mans_twin(),
        doxxing_monitor(),
        panic_broadcast(),
        evidence_archive_signal(),
        evidence_archive_telegram(),
        allow_list_monthly_review(),
        comms_audit_weekly(),
        signal_disappearing_bulk(),
        discord_keyword_defuse(),
        kick_chat_moderator(),
        vpn_disconnect_alert(),
        tor_circuit_rotate_reminder(),
        travel_mode_friday_prep(),
        key_rotation_quarterly(),
        encrypted_archive_pipe(),
    ]
}

// ── Signal auto-reply (existing) ───────────────────────────────

fn signal_relay() -> Recipe {
    Recipe {
        id: "signal-relay".into(),
        name: "Signal Auto-Reply".into(),
        description: "Reply to Signal messages with a fixed text. Local-first.".into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["signal".into(), "auto-reply".into(), "privacy".into()],
        connectors_used: vec!["connector-signal".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number (E.164)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Format: +14155550100".into()),
            },
            InputField {
                id: "reply_text".into(),
                label: "Auto-reply text".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("I'll get back to you soon.")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-signal".into(),
                config: json!({ "number": "${signal_number}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "signal-auto-reply"

[trigger]
type = "ConnectorEvent"
connector = "connector-signal"
event = "message_received"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${trigger.from}"
text = "${reply_text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Auto-replies to incoming Signal messages with a fixed text. Runs locally.".into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Dead man's switch (webhook-arm + cron-check) ──────────────

fn dead_mans_switch() -> Recipe {
    Recipe {
        id: "dead-mans-switch".into(),
        name: "Dead Man's Switch".into(),
        description:
            "If you don't ping a URL within N hours, an emergency Signal goes to trusted contacts."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["dead-mans-switch".into(), "signal".into(), "webhook".into()],
        connectors_used: vec!["connector-signal".into(), "connector-shell".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "checkin_file".into(),
                label: "Heartbeat file path".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/heartbeat")),
                hint: Some(
                    "The ping webhook writes here. The check rule reads its mtime."
                        .into(),
                ),
            },
            InputField {
                id: "ping_slug".into(),
                label: "Ping webhook slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("dms-ping")),
                hint: Some(
                    "Becomes /webhook/${slug}. Bookmark it on every device you carry."
                        .into(),
                ),
            },
            InputField {
                id: "max_age_seconds".into(),
                label: "Max silence before alert (seconds)".into(),
                kind: FieldKind::Number,
                visibility: FieldVisibility::Required,
                default: Some(json!(86400)),
                hint: Some("86400 = 24 hours.".into()),
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
                id: "trusted_recipient".into(),
                label: "Trusted contact (number or group id)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Single recipient. Layer multiple instances of this recipe for more contacts."
                        .into(),
                ),
            },
            InputField {
                id: "alert_text".into(),
                label: "Alert message".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!(
                    "Springtale dead-man's-switch fired. I have not checked in. Please call me / contact emergency contact."
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-signal".into(),
                config: json!({ "number": "${signal_number}" }),
            }],
            rules: vec![
                RuleStep {
                    toml: r#"name = "dms-ping"

[trigger]
type = "Webhook"
path = "${ping_slug}"

[[actions]]
type = "RunShell"
command = "touch ${checkin_file}"
"#
                    .into(),
                },
                RuleStep {
                    toml: r#"name = "dms-check"

[trigger]
type = "Cron"
expression = "*/30 * * * *"

[[actions]]
type = "RunShell"
command = "test $(( $(date +%s) - $(stat -c %Y ${checkin_file} 2>/dev/null || echo 0) )) -lt ${max_age_seconds} || echo TRIGGER"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${trusted_recipient}"
text = "${alert_text}"
"#
                    .into(),
                },
            ],
            ai_config: None,
            summary: Some(
                "Two rules: a webhook refreshes the heartbeat file's mtime each time you ping; a 30-minute cron checks the file's age and Signals your trusted contact when silence crosses the threshold. The check-in and age-check use system commands, so Springtale asks you to approve them the first time they run."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Manual-touch DMS twin ──────────────────────────────────────

fn checkin_dead_mans_twin() -> Recipe {
    Recipe {
        id: "checkin-dead-mans-twin".into(),
        name: "Manual-Touch Dead Man's Switch".into(),
        description:
            "Same as dead-man's-switch but triggered by `touch ${file}` instead of a webhook."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["dead-mans-switch".into(), "signal".into(), "filewatch".into()],
        connectors_used: vec!["connector-signal".into(), "connector-shell".into(), "connector-filesystem".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "checkin_file".into(),
                label: "Heartbeat file path".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/heartbeat")),
                hint: None,
            },
            InputField {
                id: "max_age_seconds".into(),
                label: "Max silence before alert (seconds)".into(),
                kind: FieldKind::Number,
                visibility: FieldVisibility::Required,
                default: Some(json!(86400)),
                hint: None,
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
                id: "trusted_recipient".into(),
                label: "Trusted contact".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "alert_text".into(),
                label: "Alert message".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!(
                    "Springtale dead-man's-switch fired. I have not checked in."
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-filesystem".into(),
                    config: json!({ "watch_path": "${checkin_file}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "checkin-dead-mans-twin"

[trigger]
type = "Cron"
expression = "*/30 * * * *"

[[actions]]
type = "RunShell"
command = "test $(( $(date +%s) - $(stat -c %Y ${checkin_file} 2>/dev/null || echo 0) )) -lt ${max_age_seconds} || echo TRIGGER"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${trusted_recipient}"
text = "${alert_text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Variant of dead-man's-switch for users who can't expose a public webhook. Refresh the heartbeat with `touch ${checkin_file}` from cron / a desktop shortcut / a pre-commit hook. The age-check runs a system command, so Springtale asks you to approve it the first time it fires."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Doxxing monitor ────────────────────────────────────────────

fn doxxing_monitor() -> Recipe {
    Recipe {
        id: "doxxing-monitor".into(),
        name: "Doxxing Watch".into(),
        description:
            "Search your name / address / aliases on a schedule; AI flags risk; Signal alert."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["dox".into(), "presearch".into(), "signal".into(), "monitor".into()],
        connectors_used: vec!["connector-presearch".into(), "connector-signal".into()],
        ai_required: true,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "presearch_token".into(),
                label: "Presearch API token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "watch_terms".into(),
                label: "Terms to watch (comma-separated)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Your legal name, deadnames, aliases, address fragments, phone numbers. The AI flags risky exposures."
                        .into(),
                ),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "signal_recipient".into(),
                label: "Signal recipient".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-presearch".into(),
                    config: json!({ "api_token": "${presearch_token}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "doxxing-monitor"

[trigger]
type = "Cron"
expression = "0 */6 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-presearch"
action = "search"

[actions.params]
query = "${watch_terms}"
num_results = 10

[[actions]]
type = "AiComplete"
prompt = "You are a doxxing risk classifier. Look at these search results and reply with one of: SAFE (none of the matches expose personal info), WATCH (mention of name but no PII), ALERT (address/phone/aliases visible). Reply with ONLY the single word and one sentence of detail.\n\n${last_connector_output.body}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "🔍 Doxxing watch: ${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Every 6 hours: search your name + aliases, AI risk-rates the results, Signal pings with the verdict. Tune ${watch_terms} carefully — these get searched on the public web."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Panic comms broadcast ──────────────────────────────────────

fn panic_broadcast() -> Recipe {
    Recipe {
        id: "panic-broadcast".into(),
        name: "Panic Comms Broadcast".into(),
        description:
            "Hitting one URL fires a preconfigured \"I'm in trouble\" message to Signal + Telegram."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["panic".into(), "signal".into(), "telegram".into(), "webhook".into()],
        connectors_used: vec!["connector-signal".into(), "connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "panic_slug".into(),
                label: "Panic webhook slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("panic")),
                hint: Some("Becomes /webhook/${slug}. Bookmark it. Treat as a controlled secret.".into()),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "signal_recipient".into(),
                label: "Signal recipient".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "telegram_bot_token".into(),
                label: "Telegram bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "telegram_chat_id".into(),
                label: "Telegram chat id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "panic_text".into(),
                label: "Panic message".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!(
                    "PANIC: I'm in trouble. Approximate location attached in next message if available. Please call emergency contact."
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${telegram_bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "panic-broadcast"

[trigger]
type = "Webhook"
path = "${panic_slug}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "${panic_text}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${telegram_chat_id}"
text = "${panic_text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "One bookmarkable URL fires panic comms to multiple channels. Composes with the existing G5 quick-hide + vault-lock chain — hit panic, then close the app."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Evidence archive: Signal ───────────────────────────────────

fn evidence_archive_signal() -> Recipe {
    Recipe {
        id: "evidence-archive-signal".into(),
        name: "Signal Evidence Archive".into(),
        description:
            "Per-sender append-only archive of incoming Signal messages for evidence preservation."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["signal".into(), "archive".into(), "evidence".into()],
        connectors_used: vec!["connector-signal".into(), "connector-filesystem".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "watch_sender".into(),
                label: "Sender to archive (E.164 or group id)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Layer multiple instances of this recipe for multiple senders / groups."
                        .into(),
                ),
            },
            InputField {
                id: "archive_dir".into(),
                label: "Archive directory".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/archive/signal")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-filesystem".into(),
                    config: json!({ "watch_path": "${archive_dir}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "evidence-archive-signal"

[trigger]
type = "ConnectorEvent"
connector = "connector-signal"
event = "message_received"

[[conditions]]
type = "FieldEquals"
field = "from"
value = "${watch_sender}"

[[actions]]
type = "WriteFile"
destination = "${archive_dir}/${trigger.from}/${trigger.timestamp}.json"
content = "${trigger.message}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Every incoming Signal message from the watched sender is written to a per-sender, per-timestamp file. Append-only by filename — nothing overwritten."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Evidence archive: Telegram ─────────────────────────────────

fn evidence_archive_telegram() -> Recipe {
    Recipe {
        id: "evidence-archive-telegram".into(),
        name: "Telegram Evidence Archive".into(),
        description: "Same shape as the Signal archive recipe but for Telegram chats.".into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["telegram".into(), "archive".into(), "evidence".into()],
        connectors_used: vec!["connector-telegram".into(), "connector-filesystem".into()],
        ai_required: false,
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
                id: "watch_chat_id".into(),
                label: "Chat id to archive".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "archive_dir".into(),
                label: "Archive directory".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/archive/telegram")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-filesystem".into(),
                    config: json!({ "watch_path": "${archive_dir}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "evidence-archive-telegram"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message"

[[conditions]]
type = "FieldEquals"
field = "chat_id"
value = "${watch_chat_id}"

[[actions]]
type = "WriteFile"
destination = "${archive_dir}/${trigger.chat_id}/${trigger.message_id}.json"
content = "${trigger.text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Append-only archive of every message in the watched Telegram chat.".into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Allow-list monthly review ──────────────────────────────────

fn allow_list_monthly_review() -> Recipe {
    Recipe {
        id: "allow-list-monthly-review".into(),
        name: "Allow-List Monthly Review".into(),
        description:
            "1st of the month: Telegram ping reminding you to review connector / host allow-lists."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["audit".into(), "telegram".into(), "monthly".into()],
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
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "allow-list-monthly-review"

[trigger]
type = "Cron"
expression = "0 10 1 * *"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "🛡 Monthly review: which connectors / hosts / contacts have access? Drop any you don't recognise. Open Springtale → Connectors."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "First of the month at 10am: nudge to audit your own allow-lists. Pure reminder; the audit happens in your UI."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Weekly comms audit ─────────────────────────────────────────

fn comms_audit_weekly() -> Recipe {
    Recipe {
        id: "comms-audit-weekly".into(),
        name: "Weekly Comms Audit".into(),
        description:
            "Sunday-morning summary of your own bot footprint, delivered via Telegram."
                .into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["audit".into(), "transparency".into(), "telegram".into()],
        connectors_used: vec!["connector-telegram".into()],
        ai_required: false,
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
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "comms-audit-weekly"

[trigger]
type = "Cron"
expression = "0 11 * * 0"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "📋 Weekly comms audit — time to review your connectors, paired devices, and access list in Settings → Safety. Look for anything you don't recognize and revoke it."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Sunday 11am: enumerate your configured connectors, summarise into one Telegram message. Transparency about your own footprint."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Signal disappearing-timer sweep ───────────────────────────

fn signal_disappearing_bulk() -> Recipe {
    Recipe {
        id: "signal-disappearing-bulk".into(),
        name: "Signal Disappearing-Timer Sweep".into(),
        description:
            "1st of every month: re-apply a 7-day disappearing-message timer to every Signal thread."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["signal".into(), "disappearing".into(), "monthly".into()],
        connectors_used: vec!["connector-signal".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "ttl_seconds".into(),
                label: "Disappearing-timer TTL (seconds)".into(),
                kind: FieldKind::Number,
                visibility: FieldVisibility::Optional,
                default: Some(json!(604800)),
                hint: Some("604800 = 7 days.".into()),
            },
            InputField {
                id: "target_thread".into(),
                label: "Thread to reset (number or group id)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Layer multiple instances for multiple threads; signal-cli supports per-thread timers only."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-signal".into(),
                config: json!({ "number": "${signal_number}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "signal-disappearing-bulk"

[trigger]
type = "Cron"
expression = "0 4 1 * *"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "set_disappearing_timer"

[actions.params]
recipient = "${target_thread}"
expires_in_seconds = "${ttl_seconds}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "1st of the month at 4am: re-apply the disappearing-message TTL on the target Signal thread. Pushes drift back to safe defaults."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Discord keyword defuse ─────────────────────────────────────

fn discord_keyword_defuse() -> Recipe {
    Recipe {
        id: "discord-keyword-defuse".into(),
        name: "Discord Keyword Defuse".into(),
        description:
            "Auto-remove messages containing a configurable slur / keyword from a Discord channel."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["discord".into(), "moderation".into()],
        connectors_used: vec!["connector-discord".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "bot_token".into(),
                label: "Discord bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Bot needs Manage Messages permission on the target channel."
                        .into(),
                ),
            },
            InputField {
                id: "application_id".into(),
                label: "Discord application id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "channel_id".into(),
                label: "Channel id to moderate".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "keyword".into(),
                label: "Keyword to remove".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Case-sensitive contains-match. For multiple keywords, layer multiple instances of this recipe."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-discord".into(),
                config: json!({
                    "bot_token": "${bot_token}",
                    "application_id": "${application_id}",
                    "enable_message_content": true
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "discord-keyword-defuse"

[trigger]
type = "ConnectorEvent"
connector = "connector-discord"
event = "message_received"

[[conditions]]
type = "FieldEquals"
field = "channel_id"
value = "${channel_id}"

[[conditions]]
type = "Contains"
field = "content"
value = "${keyword}"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "delete_message"

[actions.params]
channel_id = "${trigger.channel_id}"
message_id = "${trigger.message_id}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Auto-deletes messages on a watched channel that contain the configured keyword. Bot needs Manage Messages."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Kick chat moderator ────────────────────────────────────────

fn kick_chat_moderator() -> Recipe {
    Recipe {
        id: "kick-chat-moderator".into(),
        name: "Kick Chat Moderator".into(),
        description:
            "AI-assisted moderation reply in your Kick chat. Threshold + tone are tunable."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["kick".into(), "moderation".into(), "ai".into(), "streamer".into()],
        connectors_used: vec!["connector-kick".into()],
        ai_required: true,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "channel_slug".into(),
                label: "Kick channel slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "webhook_secret".into(),
                label: "Kick webhook secret".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "sensitivity".into(),
                label: "Sensitivity".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "high".into(), label: "High — flag harassment + spam".into() },
                        SelectOption { value: "medium".into(), label: "Medium — slurs + threats".into() },
                        SelectOption { value: "low".into(), label: "Low — only severe content".into() },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("medium")),
                hint: None,
            },
            InputField {
                id: "warning_text".into(),
                label: "Warning reply text".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!(
                    "Heads-up: that message looks rule-breaking. Stick to chat rules in /info."
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-kick".into(),
                config: json!({
                    "channel_slug": "${channel_slug}",
                    "webhook_secret": "${webhook_secret}"
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "kick-chat-moderator"

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "chat_message"

[[actions]]
type = "AiComplete"
prompt = "You are a chat moderator with ${sensitivity} sensitivity. Reply with exactly the string `WARN` if this Kick chat message breaks community rules (slurs, harassment, threats, doxxing, spam). Reply with `OK` otherwise.\n\nMessage: ${trigger.content}"

[[actions]]
type = "RunConnector"
connector = "connector-kick"
action = "send_chat"

[actions.params]
channel_id = "${trigger.broadcaster.user_id}"
message = "${warning_text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "AI classifies every chat message; when the classifier says WARN, the bot posts a public warning. Off by default — start in dry-run by setting warning_text to a private channel first."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── VPN disconnect alert ───────────────────────────────────────

fn vpn_disconnect_alert() -> Recipe {
    Recipe {
        id: "vpn-disconnect-alert".into(),
        name: "VPN Disconnect Alert".into(),
        description:
            "Per-minute VPN watch via shell; Telegram ping when the tunnel goes down."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["vpn".into(), "shell".into(), "telegram".into()],
        connectors_used: vec!["connector-shell".into(), "connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "tun_interface".into(),
                label: "Tunnel interface to watch".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("tun0")),
                hint: Some("e.g. tun0, wg0, utun1. Linux iproute2 / macOS ifconfig syntax.".into()),
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
            InputField {
                id: "schedule".into(),
                label: "Check schedule".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "* * * * *".into(), label: "Every minute".into() },
                        SelectOption { value: "*/5 * * * *".into(), label: "Every 5 minutes".into() },
                    ],
                },
                visibility: FieldVisibility::Advanced,
                default: Some(json!("* * * * *")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "vpn-disconnect-alert"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunShell"
command = "ip link show ${tun_interface} >/dev/null 2>&1 && echo UP || echo DOWN"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "🔐 VPN monitor ran for ${tun_interface}. Live up/down status needs shell access — approve it in Settings → Safety to enable real alerts."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Cron polls the tunnel interface every minute; Telegram message reflects state. Future enhancement: only ping on state-change once the rule engine exposes conditional outputs."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Tor circuit rotation reminder ──────────────────────────────

fn tor_circuit_rotate_reminder() -> Recipe {
    Recipe {
        id: "tor-circuit-rotate-reminder".into(),
        name: "Tor Circuit Rotation Reminder".into(),
        description:
            "Every 4 hours during waking time-window: nudge to restart Tor / rotate identity."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["tor".into(), "hygiene".into(), "notify".into()],
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
                            value: "0 8-22/4 * * *".into(),
                            label: "Every 4h, 8am–10pm".into(),
                        },
                        SelectOption {
                            value: "0 */6 * * *".into(),
                            label: "Every 6h, all day".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("0 8-22/4 * * *")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "tor-circuit-rotate-reminder"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "Notify"
title = "Circuit refresh"
body = "Restart Tor / rotate identity if you've been on long-running sessions."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Per Tor Project guidance: rotate circuits regularly. This is the reminder side; the actual rotation is yours to do."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Friday travel-mode prep ───────────────────────────────────

fn travel_mode_friday_prep() -> Recipe {
    Recipe {
        id: "travel-mode-friday-prep".into(),
        name: "Friday Travel-Mode Prep".into(),
        description:
            "5pm Friday: nudge to set up travel mode — disguise, vault, panic-tap, kit checklist."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["travel".into(), "notify".into(), "weekly".into()],
        connectors_used: vec![],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![RuleStep {
                toml: r#"name = "travel-mode-friday-prep"

[trigger]
type = "Cron"
expression = "0 17 * * 5"

[[actions]]
type = "Notify"
title = "Travel mode?"
body = "Lock vault • set disguise • test panic-tap • travel kit checklist."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Pre-weekend nudge that lines up with the existing G5 safety surface (disguise, panic-tap, content protection). Compose with travel-prepare CLI for the actual switch."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Quarterly key rotation ────────────────────────────────────

fn key_rotation_quarterly() -> Recipe {
    Recipe {
        id: "key-rotation-quarterly".into(),
        name: "Quarterly Key Rotation".into(),
        description:
            "Once a quarter: Telegram ping to rotate keys (Signal safety numbers, Nostr keys, app passwords, PATs)."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["rotation".into(), "hygiene".into(), "telegram".into()],
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
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "key-rotation-quarterly"

[trigger]
type = "Cron"
expression = "0 10 1 1,4,7,10 *"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "🔑 Quarterly key rotation: Signal safety numbers • Nostr keys • Bluesky app passwords • GitHub PATs • SSH keys. Roll anything ≥90d old."
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Quarterly Telegram nudge to rotate long-lived secrets. Per EFF hygiene guidance."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── Encrypted archive pipe ────────────────────────────────────

fn encrypted_archive_pipe() -> Recipe {
    Recipe {
        id: "encrypted-archive-pipe".into(),
        name: "Encrypted Archive Pipe".into(),
        description:
            "POST `{ \"payload\": \"...\" }` to a webhook; payload gets GPG-encrypted to a local file."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::SafetyPrivacy,
        tags: vec!["archive".into(), "gpg".into(), "webhook".into(), "local-first".into()],
        connectors_used: vec!["connector-shell".into(), "connector-filesystem".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "archive_slug".into(),
                label: "Webhook slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("archive")),
                hint: Some("Becomes /webhook/${slug}. Treat as a controlled secret.".into()),
            },
            InputField {
                id: "gpg_key_id".into(),
                label: "GPG recipient key id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "The public key fingerprint to encrypt to. Must be in the local gpg keyring."
                        .into(),
                ),
            },
            InputField {
                id: "archive_dir".into(),
                label: "Archive directory".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/encrypted")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-filesystem".into(),
                config: json!({ "watch_path": "${archive_dir}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "encrypted-archive-pipe"

[trigger]
type = "Webhook"
path = "${archive_slug}"

[[actions]]
type = "RunShell"
command = "echo '${trigger.body}' | gpg --batch --yes --encrypt --recipient ${gpg_key_id} --output ${archive_dir}/$(date +%s).gpg"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "POST a payload to your webhook; the recipe GPG-encrypts it to a timestamped file on local disk. Re-encrypt later with `gpg --decrypt`. Useful for ephemeral leakage / evidence pipes. Encryption runs a system command (gpg), so Springtale asks you to approve it the first time it fires."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}
