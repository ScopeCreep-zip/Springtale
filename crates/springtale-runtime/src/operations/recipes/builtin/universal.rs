//! Universal modular recipes — the Phase A + B bundle.
//!
//! Eight parametrized shapes that compose existing connector primitives
//! via the new `Action::Extract` + `Action::Dedupe` variants:
//!
//! ### Phase A — HTTP-only (no chromiumoxide)
//! 1. [`scheduled_web_fetch`] — fetch any URL on a schedule, extract,
//!    optionally AI-summarise, send to any messaging channel.
//! 2. [`rss_broadcast`] — RSS/Atom/JSON Feed → messaging channel.
//! 3. [`api_poll_conditional`] — JSON API → JSONPath → message.
//! 4. [`calendar_feed_reminder`] — iCal feed → messaging reminder.
//! 5. [`webhook_fanout_multi`] — inbound webhook → fan out to N
//!    messaging channels.
//! 6. [`cross_channel_broadcast`] — connector-event source → fan out
//!    to N messaging channels.
//!
//! ### Phase B — browser-based (chromiumoxide page-function actions)
//! 7. [`browser_page_function`] — JS-rendered page → CSS extract →
//!    messaging. The "scrape a SPA" general shape.
//! 8. [`page_change_watcher`] — JS-rendered page → narrowed CSS region
//!    → `PageDiff` hash → dedupe → alert. The "tell me when X changes
//!    on this page" general shape.
//!
//! Each replaces ~3–5 content-specific recipes (weather, stocks, HN,
//! Reddit, etc.) with a single parametrized shape. The v2 plan's
//! design principle is "the source should be configurable" — these
//! recipes have URL / extraction / destination all as user inputs,
//! so a Sacramento-weather bot and a Bitcoin-price bot are the same
//! recipe with different fills.

use serde_json::json;

use super::super::types::{
    ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        scheduled_web_fetch(),
        rss_broadcast(),
        api_poll_conditional(),
        calendar_feed_reminder(),
        webhook_fanout_multi(),
        cross_channel_broadcast(),
        browser_page_function(),
        page_change_watcher(),
    ]
}

// ── #1 scheduled-web-fetch ────────────────────────────────────────

fn scheduled_web_fetch() -> Recipe {
    Recipe {
        id: "scheduled-web-fetch".into(),
        name: "Scheduled Web Fetch".into(),
        description:
            "Fetch any URL on a schedule, extract data via JSONPath, send to Telegram. \
             Replaces weather digests, stock trackers, crypto trackers, HN/Reddit \
             aggregations — anything that's 'poll URL X, tell me Y'."
                .into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Web,
        tags: vec![
            "scheduled".into(),
            "http".into(),
            "extract".into(),
            "universal".into(),
        ],
        connectors_used: vec!["connector-http".into(), "connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "url".into(),
                label: "URL to fetch".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Example: https://wttr.in/Sacramento?format=j1 for weather, \
                     https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd for crypto"
                        .into(),
                ),
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Host portion only (e.g. `wttr.in`).".into()),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Cron,
                visibility: FieldVisibility::Required,
                default: Some(json!("0 7 * * *")),
                hint: Some("Cron expression. Default: daily at 7am.".into()),
            },
            InputField {
                id: "jsonpath_field".into(),
                label: "JSONPath for the value to extract".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("$.current_condition[0].weatherDesc[0].value")),
                hint: Some(
                    "RFC 9535 JSONPath. Resolves against the response body."
                        .into(),
                ),
            },
            InputField {
                id: "bot_token".into(),
                label: "Telegram bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("From @BotFather.".into()),
            },
            InputField {
                id: "chat_id".into(),
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination from the dropdown. Click 🎯 Onboard \
                     to register a new chat by sending /start to your bot."
                        .into(),
                ),
            },
            InputField {
                id: "message_template".into(),
                label: "Message template".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("📊 ${last_extract_output.value}")),
                hint: Some(
                    "Use `${last_extract_output.value}` for the extracted JSONPath value."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-http".into(),
                    config: json!({ "allowed_hosts": ["${host}"] }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "scheduled-web-fetch"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"
[actions.params]
url = "${url}"

[[actions]]
type = "Extract"
source = "last_connector_output.body"
[actions.kind]
kind = "json_path"
[actions.kind.schema]
value = "${jsonpath_field}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "${message_template}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Polls a URL on a schedule, extracts a field via JSONPath, sends \
                 to Telegram. Use as a building block for daily digests."
                    .into(),
            ),
        },
    }
}

// ── #2 rss-broadcast ──────────────────────────────────────────────

fn rss_broadcast() -> Recipe {
    Recipe {
        id: "rss-broadcast".into(),
        name: "RSS Broadcast".into(),
        description:
            "Watch any RSS/Atom feed; for each new entry, send a Telegram message. \
             Replaces YouTube channel alerts, GitHub release watches, blog feeds, \
             Nitter/RSSHub bridges."
                .into(),
        icon_id: "rss".into(),
        category: RecipeCategory::Web,
        tags: vec![
            "rss".into(),
            "atom".into(),
            "scheduled".into(),
            "universal".into(),
        ],
        connectors_used: vec!["connector-http".into(), "connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "rss_url".into(),
                label: "RSS / Atom feed URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Example: https://news.ycombinator.com/rss for HN, \
                     https://www.youtube.com/feeds/videos.xml?channel_id=UC… for YouTube."
                        .into(),
                ),
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Host portion only (e.g. `news.ycombinator.com`).".into()),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Cron,
                visibility: FieldVisibility::Required,
                default: Some(json!("*/15 * * * *")),
                hint: Some("Cron expression. Default: every 15 minutes.".into()),
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
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination. Click 🎯 Onboard to register a new \
                     chat by sending /start to your bot."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-http".into(),
                    config: json!({ "allowed_hosts": ["${host}"] }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "rss-broadcast"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"
[actions.params]
url = "${rss_url}"

[[actions]]
type = "Extract"
source = "last_connector_output.body"
[actions.kind]
kind = "feed"

[[actions]]
type = "Dedupe"
key = "${last_extract_output.entries.0.id}"
bucket = "rss-broadcast"
history = 10000

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "📰 ${last_extract_output.entries.0.title}\n${last_extract_output.entries.0.url}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Pulls an RSS/Atom feed on a schedule; dedupes by entry id so each \
                 new entry alerts exactly once."
                    .into(),
            ),
        },
    }
}

// ── #3 api-poll-conditional ───────────────────────────────────────

fn api_poll_conditional() -> Recipe {
    Recipe {
        id: "api-poll-conditional".into(),
        name: "API Poll with Alert".into(),
        description:
            "Poll any JSON API on a schedule, extract a value via JSONPath, alert \
             on every fire. Replaces price-tracker / stock-watcher recipes. The \
             threshold-comparison version lands when conditional dispatch is \
             surfaced in the recipe builder (Phase B)."
                .into(),
        icon_id: "chart".into(),
        category: RecipeCategory::Web,
        tags: vec![
            "api".into(),
            "jsonpath".into(),
            "scheduled".into(),
            "universal".into(),
        ],
        connectors_used: vec!["connector-http".into(), "connector-signal".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "api_url".into(),
                label: "JSON API URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Example: https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd"
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
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Cron,
                visibility: FieldVisibility::Required,
                default: Some(json!("*/10 * * * *")),
                hint: Some("Cron expression. Default: every 10 minutes.".into()),
            },
            InputField {
                id: "jsonpath_value".into(),
                label: "JSONPath expression".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("$.bitcoin.usd")),
                hint: Some("Path into the response body to extract.".into()),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("E.164 format (e.g. +14155550100).".into()),
            },
            InputField {
                id: "signal_recipient".into(),
                label: "Send to (Signal)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-signal".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a Signal recipient or group. Connector-signal's \
                     `discover_destinations` action lists everything visible \
                     to the signal-cli daemon."
                        .into(),
                ),
            },
            InputField {
                id: "alert_template".into(),
                label: "Alert message".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("📈 Current value: ${last_extract_output.value}")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-http".into(),
                    config: json!({ "allowed_hosts": ["${host}"] }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "api-poll-conditional"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"
[actions.params]
url = "${api_url}"

[[actions]]
type = "Extract"
source = "last_connector_output.body"
[actions.kind]
kind = "json_path"
[actions.kind.schema]
value = "${jsonpath_value}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"
[actions.params]
to = "${signal_recipient}"
text = "${alert_template}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Polls a JSON API on a schedule, extracts a field via JSONPath, \
                 alerts via Signal. Use for price tracking, stock watching, \
                 service health monitoring."
                    .into(),
            ),
        },
    }
}

// ── #4 calendar-feed-reminder ─────────────────────────────────────

fn calendar_feed_reminder() -> Recipe {
    Recipe {
        id: "calendar-feed-reminder".into(),
        name: "Calendar Feed Reminder".into(),
        description:
            "Pull an iCalendar (.ics) feed on a schedule; dedupe by event UID so \
             each event alerts once. For Google Calendar, Apple Calendar, ProtonMail \
             Calendar, FastMail Calendar — any provider that exposes an .ics URL."
                .into(),
        icon_id: "calendar".into(),
        category: RecipeCategory::Daily,
        tags: vec![
            "calendar".into(),
            "ical".into(),
            "scheduled".into(),
            "universal".into(),
        ],
        connectors_used: vec!["connector-http".into(), "connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "ical_url".into(),
                label: "iCalendar (.ics) feed URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Google Calendar: Settings → Integrate → Secret address in iCal format."
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
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Cron,
                visibility: FieldVisibility::Required,
                default: Some(json!("0 * * * *")),
                hint: Some("Cron expression. Default: hourly.".into()),
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
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination. Click 🎯 Onboard to register a new \
                     chat by sending /start to your bot."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-http".into(),
                    config: json!({ "allowed_hosts": ["${host}"] }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "calendar-feed-reminder"

[trigger]
type = "Cron"
expression = "${schedule}"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"
[actions.params]
url = "${ical_url}"

[[actions]]
type = "Extract"
source = "last_connector_output.body"
[actions.kind]
kind = "ical"

[[actions]]
type = "Dedupe"
key = "${last_extract_output.events.0.uid}"
bucket = "calendar-feed-reminder"
history = 5000

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "📅 ${last_extract_output.events.0.summary}\nStarts: ${last_extract_output.events.0.starts_at}\nLocation: ${last_extract_output.events.0.location}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Pulls an iCalendar feed on a schedule. Each new event (by UID) \
                 sends a Telegram reminder with summary, start time, location."
                    .into(),
            ),
        },
    }
}

// ── #5 webhook-fanout-multi ───────────────────────────────────────

fn webhook_fanout_multi() -> Recipe {
    Recipe {
        id: "webhook-fanout-multi".into(),
        name: "Webhook Fan-out".into(),
        description:
            "An inbound webhook fans out to multiple messaging channels. Wire \
             external systems (GitHub, CI/CD, monitoring) to a single Springtale \
             URL and have them broadcast to your Telegram + Signal + Discord at \
             once."
                .into(),
        icon_id: "broadcast".into(),
        category: RecipeCategory::Web,
        tags: vec![
            "webhook".into(),
            "fanout".into(),
            "universal".into(),
        ],
        connectors_used: vec![
            "connector-telegram".into(),
            "connector-signal".into(),
        ],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "webhook_slug".into(),
                label: "Webhook slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("incoming")),
                hint: Some(
                    "Becomes /webhook/${slug}. POST to that URL to fire the rule."
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
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination. Click 🎯 Onboard to register a new \
                     chat by sending /start to your bot."
                        .into(),
                ),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("E.164 format.".into()),
            },
            InputField {
                id: "signal_recipient".into(),
                label: "Send to (Signal)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-signal".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a Signal recipient or group. Connector-signal's \
                     `discover_destinations` lists what the signal-cli \
                     daemon can reach."
                        .into(),
                ),
            },
            InputField {
                id: "message_template".into(),
                label: "Message template".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("📡 ${trigger.message}")),
                hint: Some(
                    "Refer to incoming webhook fields as `${trigger.field_name}`."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "webhook-fanout-multi"

[trigger]
type = "Webhook"
path = "${webhook_slug}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "${message_template}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"
[actions.params]
to = "${signal_recipient}"
text = "${message_template}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Inbound POST to /webhook/${webhook_slug} fans out to Telegram + \
                 Signal with a templated message that reads the webhook body."
                    .into(),
            ),
        },
    }
}

// ── #6 cross-channel-broadcast ────────────────────────────────────

fn cross_channel_broadcast() -> Recipe {
    Recipe {
        id: "cross-channel-broadcast".into(),
        name: "Cross-Channel Broadcast".into(),
        description:
            "When a connector emits an event (Telegram message, Discord post, etc.), \
             fan out to multiple other messaging channels. Generalises the existing \
             nostr↔bluesky relay pattern."
                .into(),
        icon_id: "broadcast".into(),
        category: RecipeCategory::Messaging,
        tags: vec![
            "cross-post".into(),
            "fanout".into(),
            "universal".into(),
        ],
        connectors_used: vec![
            "connector-telegram".into(),
            "connector-signal".into(),
        ],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "source_connector".into(),
                label: "Source connector".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption {
                            value: "connector-telegram".into(),
                            label: "Telegram".into(),
                        },
                        SelectOption {
                            value: "connector-discord".into(),
                            label: "Discord".into(),
                        },
                        SelectOption {
                            value: "connector-slack".into(),
                            label: "Slack".into(),
                        },
                        SelectOption {
                            value: "connector-signal".into(),
                            label: "Signal".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("connector-telegram")),
                hint: Some("Which connector fires the trigger.".into()),
            },
            InputField {
                id: "source_event".into(),
                label: "Source event name".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("message")),
                hint: Some(
                    "Event name as declared by the source connector (e.g. `message`)."
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
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination. Click 🎯 Onboard to register a new \
                     chat by sending /start to your bot."
                        .into(),
                ),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("E.164 format.".into()),
            },
            InputField {
                id: "signal_recipient".into(),
                label: "Send to (Signal)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-signal".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a Signal recipient or group. Connector-signal's \
                     `discover_destinations` lists what the signal-cli \
                     daemon can reach."
                        .into(),
                ),
            },
            InputField {
                id: "message_template".into(),
                label: "Message template".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("🔁 ${trigger.text}")),
                hint: Some(
                    "Use trigger fields like `${trigger.text}` for the source's message body."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "cross-channel-broadcast"

[trigger]
type = "ConnectorEvent"
connector = "${source_connector}"
event = "${source_event}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "${message_template}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"
[actions.params]
to = "${signal_recipient}"
text = "${message_template}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Listens for an event from one connector and re-broadcasts it to \
                 Telegram + Signal with a templated message."
                    .into(),
            ),
        },
    }
}

// ── #7 browser-page-function ─────────────────────────────────────
//
// JS-rendered page → wait-for-selector → get_html → CSS-schema
// extraction → Telegram. The "scrape a SPA with stable selectors"
// general shape. Power difficulty per `feedback_no_ban_risk`: the
// description warns about anti-bot risk so users self-select.

fn browser_page_function() -> Recipe {
    Recipe {
        id: "browser-page-function".into(),
        name: "Browser Page Function".into(),
        description:
            "Scrape a JavaScript-rendered page on a schedule using headless Chromium. \
             Picks values out of the rendered DOM by CSS selector. \
             ⚠️ Some sites detect headless browsers and may ban your IP — \
             prefer official APIs / RSS feeds when available."
                .into(),
        icon_id: "browser".into(),
        category: RecipeCategory::Web,
        tags: vec![
            "browser".into(),
            "scrape".into(),
            "scheduled".into(),
            "universal".into(),
            "advanced".into(),
        ],
        connectors_used: vec![
            "connector-browser".into(),
            "connector-telegram".into(),
        ],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "url".into(),
                label: "Page URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Full HTTPS URL of the page to scrape.".into()),
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Host portion only (e.g. `example.com`). The browser \
                     connector's allow-list — navigation to any other host \
                     is blocked at the capability layer."
                        .into(),
                ),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Cron,
                visibility: FieldVisibility::Required,
                default: Some(json!("0 */6 * * *")),
                hint: Some(
                    "Cron expression. Default: every 6 hours. \
                     Aggressive polling raises ban risk."
                        .into(),
                ),
            },
            InputField {
                id: "wait_selector".into(),
                label: "Wait-for selector".into(),
                kind: FieldKind::CssSelector {
                    sample_url: Some("url".into()),
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("main")),
                hint: Some(
                    "CSS selector to wait for before extracting — the \
                     marker that says \"the SPA has hydrated\". Default \
                     `main` works for most server-rendered sites."
                        .into(),
                ),
            },
            InputField {
                id: "value_selector".into(),
                label: "Value selector".into(),
                kind: FieldKind::CssSelector {
                    sample_url: Some("url".into()),
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "CSS selector for the value to extract. The recipe \
                     binds this to the schema field `value` — the message \
                     template references it as `${last_extract_output.value}`."
                        .into(),
                ),
            },
            InputField {
                id: "bot_token".into(),
                label: "Telegram bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("From @BotFather.".into()),
            },
            InputField {
                id: "chat_id".into(),
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination. Click 🎯 Onboard to register a new \
                     chat by sending /start to your bot."
                        .into(),
                ),
            },
            InputField {
                id: "message_template".into(),
                label: "Message template".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("📄 ${last_extract_output.value}")),
                hint: Some(
                    "`${last_extract_output.value}` is the extracted text."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-browser".into(),
                    config: json!({
                        "allowed_domains": ["${host}"],
                        "disable_telemetry": true,
                        "stealth_profile": "off",
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "browser-page-function"

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
action = "wait_for_selector"
[actions.params]
selector = "${wait_selector}"
timeout_ms = 8000

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "get_html"
[actions.params]

[[actions]]
type = "Extract"
source = "last_connector_output.html"
[actions.kind]
kind = "css"
[actions.kind.schema]
value = "${value_selector}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "${message_template}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Renders a page in headless Chromium, extracts a value by CSS \
                 selector, posts to Telegram. Replaces hand-rolled Playwright \
                 scripts for stable SPAs."
                    .into(),
            ),
        },
    }
}

// ── #8 page-change-watcher ───────────────────────────────────────
//
// JS-rendered page → narrowed region → PageDiff hash → dedupe → alert.
// Fires only when the watched region's normalised content actually
// changes (script/style stripped, whitespace collapsed, blake3 hash).
// Uses the dedupe primitive's "fresh" gate to avoid alert spam.

fn page_change_watcher() -> Recipe {
    Recipe {
        id: "page-change-watcher".into(),
        name: "Page Change Watcher".into(),
        description:
            "Watch a page region and alert on change. Renders the page in \
             headless Chromium, narrows to a CSS region, hashes the normalised \
             text (script/style stripped), dedupes on the hash — alerts fire \
             only when the region actually changed. \
             ⚠️ Anti-bot detection risk; prefer official feeds where possible."
                .into(),
        icon_id: "eye".into(),
        category: RecipeCategory::Web,
        tags: vec![
            "browser".into(),
            "watch".into(),
            "diff".into(),
            "scheduled".into(),
            "universal".into(),
            "advanced".into(),
        ],
        connectors_used: vec![
            "connector-browser".into(),
            "connector-telegram".into(),
        ],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "url".into(),
                label: "Page URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Full HTTPS URL of the page to watch.".into()),
            },
            InputField {
                id: "host".into(),
                label: "Allowed host".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Host portion only.".into()),
            },
            InputField {
                id: "schedule".into(),
                label: "Schedule".into(),
                kind: FieldKind::Cron,
                visibility: FieldVisibility::Required,
                default: Some(json!("*/30 * * * *")),
                hint: Some(
                    "Cron expression. Default: every 30 minutes — a \
                     conservative cadence that respects most sites."
                        .into(),
                ),
            },
            InputField {
                id: "region_selector".into(),
                label: "Region CSS selector".into(),
                kind: FieldKind::CssSelector {
                    sample_url: Some("url".into()),
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("main")),
                hint: Some(
                    "CSS selector narrowing the watched area. Picking a tight \
                     region prevents nav / footer / ad changes from triggering \
                     false-positive alerts."
                        .into(),
                ),
            },
            // History buffer hard-coded at 50 in the rule TOML —
            // enough to remember every poll over a couple of days
            // at the default 30-min cadence without unbounded
            // growth. Power users fork the recipe to tweak.
            InputField {
                id: "bot_token".into(),
                label: "Telegram bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("From @BotFather.".into()),
            },
            InputField {
                id: "chat_id".into(),
                label: "Send to (Telegram)".into(),
                kind: FieldKind::WorkspaceTarget {
                    connector: "connector-telegram".into(),
                    kinds: vec![],
                },
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Pick a destination. Click 🎯 Onboard to register a new \
                     chat by sending /start to your bot."
                        .into(),
                ),
            },
            InputField {
                id: "alert_template".into(),
                label: "Alert template".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("🔔 Change detected on ${url}")),
                hint: Some(
                    "Fires only when the region's hash changed. \
                     `${last_extract_output.hash}` is the new digest."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-browser".into(),
                    config: json!({
                        "allowed_domains": ["${host}"],
                        "disable_telemetry": true,
                        "stealth_profile": "off",
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-telegram".into(),
                    config: json!({ "bot_token": "${bot_token}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "page-change-watcher"

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
action = "wait_for_selector"
[actions.params]
selector = "${region_selector}"
timeout_ms = 8000

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "get_html"
[actions.params]

[[actions]]
type = "Extract"
source = "last_connector_output.html"
[actions.kind]
kind = "css"
[actions.kind.schema]
region = "${region_selector}"

[[actions]]
type = "Extract"
source = "last_extract_output.region"
[actions.kind]
kind = "page_diff"

[[actions]]
type = "Dedupe"
key = "${last_extract_output.hash}"
bucket = "page-change-watcher:${url}"
history = 50

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.params]
chat_id = "${chat_id}"
text = "${alert_template}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Watches a page region; alerts on real content change. \
                 Headless Chromium renders the page, the watched region is \
                 narrowed by selector, hashed, and deduped — so the alert \
                 fires once per change, not once per poll."
                    .into(),
            ),
        },
    }
}
