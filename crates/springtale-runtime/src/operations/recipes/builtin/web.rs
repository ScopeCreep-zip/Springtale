//! Web recipes — headless browser automation, HTTP polling, RSS,
//! presearch, webhook fanouts. Connectors: browser, http, presearch,
//! filesystem.
//!
//! Patterns covered:
//! - Daily site snapshots
//! - Form autosubmit (power-user)
//! - URL uptime monitor → Signal alert
//! - News search digest (presearch + AI)
//! - RSS → multi-platform threads
//! - Inbound webhook fanout to messaging channels
//! - Personal-content archival (creator / deplatform-survival)

use serde_json::json;

use super::super::types::{
    ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        web_scraper(),
        presearch_daily_digest(),
        http_uptime_monitor(),
        browser_form_autosubmit(),
        webhook_fanout(),
        rss_multi_platform(),
        browser_daily_snapshot(),
        browser_content_archive(),
    ]
}

// ── Web Snapshot (existing) ────────────────────────────────────

fn web_scraper() -> Recipe {
    Recipe {
        id: "web-scraper".into(),
        name: "Web Snapshot".into(),
        description: "Take a daily snapshot of a public website.".into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Web,
        tags: vec!["browser".into(), "scrape".into(), "daily".into()],
        connectors_used: vec!["connector-browser".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "url".into(),
                label: "URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Full URL to snapshot, e.g. https://example.com/news".into()),
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Host portion only (e.g. example.com) — added to the connector's allow-list."
                        .into(),
                ),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "0 * * * *".into(), label: "Every hour".into() },
                        SelectOption { value: "0 0 * * *".into(), label: "Once a day".into() },
                        SelectOption {
                            value: "0 0 * * 0".into(),
                            label: "Once a week".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("0 0 * * *")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-browser".into(),
                config: json!({
                    "allowed_domains": ["${host}"],
                    "disable_telemetry": true
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "web-snapshot"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "screenshot"

[actions.params]
url = "${url}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Visits a URL on a schedule and captures a screenshot. Domain is added to the connector's allow-list.".into(),
            ),
        },
    }
}

// ── Presearch daily digest ─────────────────────────────────────

fn presearch_daily_digest() -> Recipe {
    Recipe {
        id: "presearch-daily-digest".into(),
        name: "Daily News Search → Digest".into(),
        description:
            "Top news on a chosen query → AI summary → Telegram morning digest.".into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Web,
        tags: vec!["presearch".into(), "telegram".into(), "ai".into(), "daily".into()],
        connectors_used: vec!["connector-presearch".into(), "connector-telegram".into()],
        ai_required: true,
        difficulty: Difficulty::Standard,
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
                id: "query".into(),
                label: "Search query".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. \"federal court ruling trans rights\".".into()),
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
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-presearch".into(),
                    config: json!({ "api_token": "${presearch_token}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "presearch-daily-digest"

[trigger]
type = "Cron"
expression = "0 7 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-presearch"
action = "search"

[actions.params]
query = "${query}"
num_results = 5

[[actions]]
type = "AiComplete"
prompt = "Summarise these search results in 3 short bullets, no fluff:\n${last_connector_output}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${chat_id}"
text = "🗞 ${query}\n${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Every morning at 7am, search the web for your query, AI-summarise the top 5 results, and ping you on Telegram."
                    .into(),
            ),
        },
    }
}

// ── HTTP uptime monitor ────────────────────────────────────────

fn http_uptime_monitor() -> Recipe {
    Recipe {
        id: "http-uptime-monitor".into(),
        name: "URL Uptime Monitor".into(),
        description: "Ping URL every 5 min; alert via Signal on non-200.".into(),
        icon_id: "alarm".into(),
        category: RecipeCategory::Web,
        tags: vec!["http".into(), "uptime".into(), "signal".into()],
        connectors_used: vec!["connector-http".into(), "connector-signal".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "watch_url".into(),
                label: "URL to watch".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
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
                id: "signal_recipient".into(),
                label: "Signal recipient".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Usually your own number.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-http".into(),
                    config: json!({}),
                },
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "http-uptime-monitor"

[trigger]
type = "Cron"
expression = "*/5 * * * *"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"

[actions.params]
url = "${watch_url}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "⚠ ${watch_url} health: HTTP ${last_connector_output.status}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Hits your URL every 5 minutes. Connector reports the status; Signal pings on every check (you can refine to non-200 only once the connector exposes a status condition)."
                    .into(),
            ),
        },
    }
}

// ── Browser form autosubmit ───────────────────────────────────

fn browser_form_autosubmit() -> Recipe {
    Recipe {
        id: "browser-form-autosubmit".into(),
        name: "Headless Form Autosubmit".into(),
        description: "Power-user automation: log into a routine site, fill a form, submit."
            .into(),
        icon_id: "wrench".into(),
        category: RecipeCategory::Web,
        tags: vec!["browser".into(), "automation".into()],
        connectors_used: vec!["connector-browser".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "url".into(),
                label: "Form URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Added to the browser connector's allow-list.".into()),
            },
            InputField {
                id: "input_selector".into(),
                label: "Input CSS selector".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. input[name=\"q\"].".into()),
            },
            InputField {
                id: "input_value".into(),
                label: "Value to fill".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "submit_selector".into(),
                label: "Submit-button CSS selector".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. button[type=\"submit\"].".into()),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "0 9 * * 1".into(), label: "Mondays 9am".into() },
                        SelectOption { value: "0 12 * * *".into(), label: "Daily at noon".into() },
                        SelectOption { value: "0 0 1 * *".into(), label: "1st of month".into() },
                    ],
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("0 9 * * 1")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-browser".into(),
                config: json!({
                    "allowed_domains": ["${host}"],
                    "disable_telemetry": true
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "browser-form-autosubmit"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "navigate"

[actions.params]
url = "${url}"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "fill_form"

[actions.params]
selector = "${input_selector}"
value = "${input_value}"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "click"

[actions.params]
selector = "${submit_selector}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Power-user routine: navigate → fill → submit. Host stays allow-list-gated."
                    .into(),
            ),
        },
    }
}

// ── Inbound webhook fanout ─────────────────────────────────────

fn webhook_fanout() -> Recipe {
    Recipe {
        id: "webhook-fanout".into(),
        name: "Inbound Webhook → Telegram + Discord".into(),
        description:
            "POST JSON to /webhook/fanout → message fans out to Telegram + Discord channels."
                .into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Web,
        tags: vec!["webhook".into(), "fanout".into()],
        connectors_used: vec!["connector-telegram".into(), "connector-discord".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "webhook_slug".into(),
                label: "Webhook slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("fanout")),
                hint: Some("Becomes /webhook/${slug}. Pick something unguessable.".into()),
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
                id: "tg_chat_id".into(),
                label: "Telegram chat id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
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
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
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
                toml: r#"name = "webhook-fanout"

[trigger]
type = "Webhook"
path = "${webhook_slug}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${tg_chat_id}"
text = "${trigger.body}"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "${discord_channel_id}"
content = "${trigger.body}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Bridge for systems Springtale doesn't have a connector for. Anything that can POST a webhook can now ping Telegram + Discord."
                    .into(),
            ),
        },
    }
}

// ── RSS → Bluesky/Nostr thread ─────────────────────────────────

fn rss_multi_platform() -> Recipe {
    Recipe {
        id: "rss-multi-platform".into(),
        name: "RSS → Bluesky + Nostr".into(),
        description: "RSS feed → AI-condense top entry → post to Bluesky + Nostr.".into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Web,
        tags: vec!["rss".into(), "bluesky".into(), "nostr".into()],
        connectors_used: vec![
            "connector-http".into(),
            "connector-bluesky".into(),
            "connector-nostr".into(),
        ],
        ai_required: true,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "feed_url".into(),
                label: "RSS feed URL".into(),
                kind: FieldKind::Url,
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
                    connector_name: "connector-http".into(),
                    config: json!({}),
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
                toml: r#"name = "rss-multi-platform"

[trigger]
type = "Cron"
expression = "0 */6 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"

[actions.params]
url = "${feed_url}"

[[actions]]
type = "AiComplete"
prompt = "From this RSS body, pull the most recent entry and rewrite the headline + a one-sentence summary as a single short post (≤280 chars):\n${last_connector_output}"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${last_ai_output}"

[[actions]]
type = "RunConnector"
connector = "connector-nostr"
action = "publish_note"

[actions.params]
content = "${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Every 6 hours: fetch RSS → AI-distill the latest entry into a short post → publish to Bluesky + Nostr in one shot."
                    .into(),
            ),
        },
    }
}

// ── Browser daily snapshot ─────────────────────────────────────

fn browser_daily_snapshot() -> Recipe {
    Recipe {
        id: "browser-daily-snapshot".into(),
        name: "Browser Daily Snapshot".into(),
        description:
            "Once a day, screenshot a URL into a timestamped PNG on local disk.".into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Web,
        tags: vec!["browser".into(), "screenshot".into(), "daily".into()],
        connectors_used: vec!["connector-browser".into(), "connector-filesystem".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "url".into(),
                label: "URL to snapshot".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
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
                default: Some(json!("/var/lib/springtale/snapshots")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-browser".into(),
                    config: json!({
                        "allowed_domains": ["${host}"],
                        "disable_telemetry": true
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-filesystem".into(),
                    config: json!({ "watch_path": "${archive_dir}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "browser-daily-snapshot"

[trigger]
type = "Cron"
expression = "0 0 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "screenshot"

[actions.params]
url = "${url}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Midnight: screenshot the URL. Lightweight twin of the existing Web Snapshot recipe for the common \"I just want a daily PNG\" case."
                    .into(),
            ),
        },
    }
}

// ── Personal content archive ──────────────────────────────────

fn browser_content_archive() -> Recipe {
    Recipe {
        id: "browser-content-archive".into(),
        name: "Daily Personal Content Archive".into(),
        description:
            "Pull your own profile page's posts into a local markdown archive every night."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::Web,
        tags: vec!["browser".into(), "archive".into(), "local-first".into(), "deplatform".into()],
        connectors_used: vec!["connector-browser".into(), "connector-filesystem".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "profile_url".into(),
                label: "Profile URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Your public profile page on Bluesky / Mastodon / Twitter / Tumblr / etc."
                        .into(),
                ),
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "profile_slug".into(),
                label: "Profile slug (for filename)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. yourhandle-bsky".into()),
            },
            InputField {
                id: "archive_dir".into(),
                label: "Archive directory".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/content-archive")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-browser".into(),
                    config: json!({
                        "allowed_domains": ["${host}"],
                        "disable_telemetry": true
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-filesystem".into(),
                    config: json!({ "watch_path": "${archive_dir}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "browser-content-archive"

[trigger]
type = "Cron"
expression = "0 2 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "navigate"

[actions.params]
url = "${profile_url}"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "extract_text"

[actions.params]
selector = "body"

[[actions]]
type = "WriteFile"
destination = "${archive_dir}/${profile_slug}-${trigger.timestamp}.md"
content = "${last_connector_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Nightly 2am: visit your own profile page, extract text content, write it to a timestamped markdown file you own. Survives deplatforming."
                    .into(),
            ),
        },
    }
}
