//! Messaging recipes — cross-posters, relays, community ops, channel
//! bridges. Connectors: telegram, signal, discord, slack, irc, nostr,
//! bluesky, kick.
//!
//! Patterns covered:
//! - Auto-replies (telegram-echo)
//! - Cross-network mirrors (nostr ↔ bluesky)
//! - Bridges (telegram → signal)
//! - Community ops (discord welcome, irc greeter, slack standup)
//! - Mention auto-acks
//! - Local DM archives
//! - Multi-channel /broadcast fanout
//! - Streamer go-live multi-platform announce

use serde_json::json;

use super::super::types::{
    ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        telegram_echo(),
        nostr_bluesky_relay(),
        bluesky_nostr_relay(),
        telegram_signal_bridge(),
        discord_welcome_dm(),
        discord_mention_ai_reply(),
        slack_standup_reminder(),
        irc_channel_greeter(),
        kick_stream_multipost(),
        bluesky_mention_auto_ack(),
        nostr_dm_archive(),
        telegram_cmd_broadcast(),
        bluesky_thread_scheduler(),
    ]
}

// ── Telegram echo ──────────────────────────────────────────────

fn telegram_echo() -> Recipe {
    Recipe {
        id: "telegram-echo".into(),
        name: "Telegram Echo".into(),
        description: "Reply to any message with the same text.".into(),
        icon_id: "telegram".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["telegram".into(), "echo".into()],
        connectors_used: vec!["connector-telegram".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "bot_token".into(),
                label: "Bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Create a bot with @BotFather on Telegram to get a token.".into()),
            },
            InputField {
                id: "reply_prefix".into(),
                label: "Reply prefix".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("")),
                hint: Some("Text prepended to every echo (e.g. \"You said: \").".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "telegram-echo"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat_id}"
text = "${reply_prefix}${trigger.text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Listens for new Telegram messages and replies with the same text.".into(),
            ),
        },
    }
}

// ── Nostr → Bluesky mirror ──────────────────────────────────────

fn nostr_bluesky_relay() -> Recipe {
    Recipe {
        id: "nostr-bluesky-relay".into(),
        name: "Nostr → Bluesky Mirror".into(),
        description:
            "Republish every Nostr note you write onto Bluesky, with an optional prefix."
                .into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["nostr".into(), "bluesky".into(), "crosspost".into()],
        connectors_used: vec!["connector-nostr".into(), "connector-bluesky".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "nostr_secret_key".into(),
                label: "Nostr secret key (hex)".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Your nsec/hex private key. Stored encrypted in the vault.".into()),
            },
            InputField {
                id: "nostr_relays".into(),
                label: "Nostr relay URLs (comma-separated)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("wss://relay.damus.io,wss://nos.lol")),
                hint: Some("Where to listen for your notes.".into()),
            },
            InputField {
                id: "bsky_handle".into(),
                label: "Bluesky handle".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Your @handle.bsky.social (no leading @).".into()),
            },
            InputField {
                id: "bsky_app_password".into(),
                label: "Bluesky app password".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some(
                    "Generate at bsky.app → Settings → App passwords. Not your main password."
                        .into(),
                ),
            },
            InputField {
                id: "prefix".into(),
                label: "Prefix to add to mirrored posts".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("")),
                hint: Some("e.g. \"[xpost from Nostr] \". Leave blank for verbatim mirror.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-nostr".into(),
                    config: json!({
                        "secret_key": "${nostr_secret_key}",
                        "relay_urls": "${nostr_relays}"
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-bluesky".into(),
                    config: json!({
                        "handle": "${bsky_handle}",
                        "app_password": "${bsky_app_password}"
                    }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "nostr-bluesky-relay"

[trigger]
type = "ConnectorEvent"
connector = "connector-nostr"
event = "note_received"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${prefix}${trigger.content}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Mirrors every note from your Nostr relays onto Bluesky in near-real-time.".into(),
            ),
        },
    }
}

// ── Bluesky → Nostr mirror ──────────────────────────────────────

fn bluesky_nostr_relay() -> Recipe {
    Recipe {
        id: "bluesky-nostr-relay".into(),
        name: "Bluesky → Nostr Mirror".into(),
        description: "Republish your Bluesky posts to Nostr to keep both feeds in sync.".into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["bluesky".into(), "nostr".into(), "crosspost".into()],
        connectors_used: vec!["connector-bluesky".into(), "connector-nostr".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "bsky_handle".into(),
                label: "Bluesky handle".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Used to filter the Jetstream firehose to your own posts.".into()),
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
                label: "Nostr secret key (hex)".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "nostr_relays".into(),
                label: "Nostr relays to publish to".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("wss://relay.damus.io,wss://nos.lol")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
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
                toml: r#"name = "bluesky-nostr-relay"

[trigger]
type = "ConnectorEvent"
connector = "connector-bluesky"
event = "own_post"

[[actions]]
type = "RunConnector"
connector = "connector-nostr"
action = "publish_note"

[actions.params]
content = "${trigger.text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Mirrors your Bluesky posts onto your configured Nostr relays. Useful when a platform might suddenly become hostile."
                    .into(),
            ),
        },
    }
}

// ── Telegram → Signal bridge ────────────────────────────────────

fn telegram_signal_bridge() -> Recipe {
    Recipe {
        id: "telegram-signal-bridge".into(),
        name: "Telegram → Signal Bridge".into(),
        description: "Forward messages from a specific Telegram chat to your Signal number."
            .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["telegram".into(), "signal".into(), "bridge".into()],
        connectors_used: vec!["connector-telegram".into(), "connector-signal".into()],
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
                hint: Some("From @BotFather. The bot must be a member of the source chat.".into()),
            },
            InputField {
                id: "source_chat_id".into(),
                label: "Source Telegram chat ID".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Numeric chat id. Use @userinfobot to look it up.".into()),
            },
            InputField {
                id: "signal_number".into(),
                label: "Your Signal number (E.164)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Format: +14155550100.".into()),
            },
            InputField {
                id: "dest_signal".into(),
                label: "Destination Signal number or group id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Recipient — usually your own number for personal backup.".into()),
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
                toml: r#"name = "telegram-signal-bridge"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message"

[[conditions]]
type = "FieldEquals"
field = "chat_id"
value = "${source_chat_id}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${dest_signal}"
text = "[tg] ${trigger.text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "One-way relay from a Telegram chat into a Signal recipient. Useful for keeping a Signal-side copy of a community chat for personal records."
                    .into(),
            ),
        },
    }
}

// ── Discord welcome DM ──────────────────────────────────────────

fn discord_welcome_dm() -> Recipe {
    Recipe {
        id: "discord-welcome-dm".into(),
        name: "Discord Welcome DM".into(),
        description: "Send a welcome message + rules link when someone joins your server."
            .into(),
        icon_id: "robot".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["discord".into(), "community".into(), "welcome".into()],
        connectors_used: vec!["connector-discord".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "bot_token".into(),
                label: "Discord bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Bot must have the GUILD_MEMBERS intent enabled.".into()),
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
                id: "welcome_text".into(),
                label: "Welcome message".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!(
                    "Welcome! Please read the server rules in #rules and introduce yourself in #intros."
                )),
                hint: Some("Sent as a DM to the new member.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-discord".into(),
                config: json!({
                    "bot_token": "${bot_token}",
                    "application_id": "${application_id}",
                    "enable_message_content": false,
                    "enable_direct_messages": true
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "discord-welcome-dm"

[trigger]
type = "ConnectorEvent"
connector = "connector-discord"
event = "member_joined"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "${trigger.user_id}"
content = "${welcome_text}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "DMs a configurable welcome message to every new server member."
                    .into(),
            ),
        },
    }
}

// ── Discord @-mention AI reply ──────────────────────────────────

fn discord_mention_ai_reply() -> Recipe {
    Recipe {
        id: "discord-mention-ai-reply".into(),
        name: "Discord Mention → AI Reply".into(),
        description: "When the bot is @-mentioned, the LLM responds with a short answer."
            .into(),
        icon_id: "robot".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["discord".into(), "ai".into(), "mention".into()],
        connectors_used: vec!["connector-discord".into()],
        ai_required: true,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "bot_token".into(),
                label: "Discord bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
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
                id: "persona".into(),
                label: "Bot persona".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "helpful".into(), label: "Helpful assistant".into() },
                        SelectOption { value: "concise".into(), label: "Concise expert".into() },
                        SelectOption { value: "warm".into(), label: "Warm + supportive".into() },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("helpful")),
                hint: Some("Shapes the system prompt the LLM sees.".into()),
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
                toml: r#"name = "discord-mention-ai-reply"

[trigger]
type = "ConnectorEvent"
connector = "connector-discord"
event = "app_mentioned"

[[actions]]
type = "AiComplete"
prompt = "You are a ${persona} chatbot. Reply briefly to: ${trigger.content}"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "${trigger.channel_id}"
content = "${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "AI-backed Discord mention bot. Tunable persona; needs an AI adapter configured."
                    .into(),
            ),
        },
    }
}

// ── Slack standup reminder ──────────────────────────────────────

fn slack_standup_reminder() -> Recipe {
    Recipe {
        id: "slack-standup-reminder".into(),
        name: "Slack Standup Reminder".into(),
        description: "Posts the standup prompt to a Slack channel every weekday at 9am.".into(),
        icon_id: "alarm".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["slack".into(), "standup".into(), "cron".into()],
        connectors_used: vec!["connector-slack".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "bot_token".into(),
                label: "Slack bot token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("xoxb-… from your Slack app's OAuth & Permissions page.".into()),
            },
            InputField {
                id: "app_token".into(),
                label: "Slack app-level token".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("xapp-… for Socket Mode.".into()),
            },
            InputField {
                id: "channel_id".into(),
                label: "Channel id".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("e.g. C01ABCDE234.".into()),
            },
            InputField {
                id: "prompt".into(),
                label: "Standup prompt".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!(
                    "Standup time. Reply in thread: 1) What did you ship yesterday? 2) What's on for today? 3) Any blockers?"
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-slack".into(),
                config: json!({
                    "bot_token": "${bot_token}",
                    "app_token": "${app_token}"
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "slack-standup-reminder"

[trigger]
type = "Cron"
expression = "0 9 * * 1-5"

[[actions]]
type = "RunConnector"
connector = "connector-slack"
action = "send_message"

[actions.params]
channel_id = "${channel_id}"
text = "${prompt}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Weekday 9am standup prompt to the channel of your choice."
                    .into(),
            ),
        },
    }
}

// ── IRC channel greeter ─────────────────────────────────────────

fn irc_channel_greeter() -> Recipe {
    Recipe {
        id: "irc-channel-greeter".into(),
        name: "IRC Channel Greeter".into(),
        description: "Welcomes new joins with a configurable greeting + rules link.".into(),
        icon_id: "robot".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["irc".into(), "community".into()],
        connectors_used: vec!["connector-irc".into()],
        ai_required: false,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "server".into(),
                label: "IRC server hostname".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("irc.libera.chat")),
                hint: None,
            },
            InputField {
                id: "port".into(),
                label: "Port".into(),
                kind: FieldKind::Number,
                visibility: FieldVisibility::Optional,
                default: Some(json!(6697)),
                hint: Some("6697 = TLS, 6667 = plaintext (don't).".into()),
            },
            InputField {
                id: "nick".into(),
                label: "Bot nick".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "channel".into(),
                label: "Channel to greet on (#foo)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: None,
            },
            InputField {
                id: "greeting".into(),
                label: "Greeting text".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!(
                    "welcome! read /topic for the rules. /msg me with questions."
                )),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-irc".into(),
                config: json!({
                    "server": "${server}",
                    "port": "${port}",
                    "nick": "${nick}",
                    "channels": ["${channel}"]
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "irc-channel-greeter"

[trigger]
type = "ConnectorEvent"
connector = "connector-irc"
event = "user_joined"

[[conditions]]
type = "FieldEquals"
field = "channel"
value = "${channel}"

[[actions]]
type = "RunConnector"
connector = "connector-irc"
action = "send_message"

[actions.params]
target = "${channel}"
text = "${trigger.nick}: ${greeting}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Greets new joiners on a channel with a configurable welcome line.".into(),
            ),
        },
    }
}

// ── Kick stream go-live multipost ───────────────────────────────

fn kick_stream_multipost() -> Recipe {
    Recipe {
        id: "kick-stream-multipost".into(),
        name: "Kick Live → Bluesky + Nostr".into(),
        description: "When you go live on Kick, announce on Bluesky + Nostr with title + link."
            .into(),
        icon_id: "globe".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["kick".into(), "bluesky".into(), "nostr".into(), "streamer".into()],
        connectors_used: vec![
            "connector-kick".into(),
            "connector-bluesky".into(),
            "connector-nostr".into(),
        ],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "channel_slug".into(),
                label: "Kick channel slug".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Your Kick username — the slug from your channel URL.".into()),
            },
            InputField {
                id: "kick_webhook_secret".into(),
                label: "Kick webhook secret".into(),
                kind: FieldKind::Secret,
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
                    connector_name: "connector-kick".into(),
                    config: json!({
                        "channel_slug": "${channel_slug}",
                        "webhook_secret": "${kick_webhook_secret}"
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
                toml: r#"name = "kick-stream-multipost"

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "🔴 LIVE on Kick: ${trigger.title} — https://kick.com/${channel_slug}"

[[actions]]
type = "RunConnector"
connector = "connector-nostr"
action = "publish_note"

[actions.params]
content = "🔴 LIVE on Kick: ${trigger.title} — https://kick.com/${channel_slug}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Cross-posts your Kick go-live announcement to Bluesky + Nostr in one move.".into(),
            ),
        },
    }
}

// ── Bluesky mention auto-ack ────────────────────────────────────

fn bluesky_mention_auto_ack() -> Recipe {
    Recipe {
        id: "bluesky-mention-auto-ack".into(),
        name: "Bluesky Mention Auto-Reply".into(),
        description: "Short polite AI reply to every mention. Off by default; opt-in tone."
            .into(),
        icon_id: "robot".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["bluesky".into(), "ai".into(), "mention".into()],
        connectors_used: vec!["connector-bluesky".into()],
        ai_required: true,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
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
                id: "tone".into(),
                label: "Reply tone".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "minimal".into(), label: "Minimal (✓)".into() },
                        SelectOption { value: "warm".into(), label: "Warm".into() },
                        SelectOption { value: "formal".into(), label: "Formal".into() },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("warm")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-bluesky".into(),
                config: json!({
                    "handle": "${bsky_handle}",
                    "app_password": "${bsky_app_password}"
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "bluesky-mention-auto-ack"

[trigger]
type = "ConnectorEvent"
connector = "connector-bluesky"
event = "mention"

[[actions]]
type = "AiComplete"
prompt = "Reply in a ${tone} tone with a one-sentence ack to: ${trigger.text}"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "reply"

[actions.params]
parent_uri = "${trigger.uri}"
parent_cid = "${trigger.cid}"
text = "${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Bot replies to mentions on Bluesky with an AI-drafted short note."
                    .into(),
            ),
        },
    }
}

// ── Nostr DM → local archive ────────────────────────────────────

fn nostr_dm_archive() -> Recipe {
    Recipe {
        id: "nostr-dm-archive".into(),
        name: "Nostr DM → Local Archive".into(),
        description:
            "Decrypted DMs flow into a timestamped JSON file. Local-first; nothing leaves the host."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["nostr".into(), "archive".into(), "local-first".into()],
        connectors_used: vec!["connector-nostr".into(), "connector-filesystem".into()],
        ai_required: false,
        difficulty: Difficulty::Standard,
        source: RecipeSource::Builtin,
        inputs: vec![
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
            InputField {
                id: "archive_dir".into(),
                label: "Archive directory".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/archive/nostr-dms")),
                hint: Some(
                    "Directory must exist + be writable. One file per DM, named by timestamp."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![
                ConnectorConfigStep {
                    connector_name: "connector-nostr".into(),
                    config: json!({
                        "secret_key": "${nostr_secret_key}",
                        "relay_urls": "${nostr_relays}"
                    }),
                },
                ConnectorConfigStep {
                    connector_name: "connector-filesystem".into(),
                    config: json!({ "watch_path": "${archive_dir}" }),
                },
            ],
            rules: vec![RuleStep {
                toml: r#"name = "nostr-dm-archive"

[trigger]
type = "ConnectorEvent"
connector = "connector-nostr"
event = "dm_received"

[[actions]]
type = "WriteFile"
destination = "${archive_dir}/${trigger.created_at}-${trigger.sender_pubkey}.json"
content = "${trigger.content}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Every incoming Nostr DM is decrypted in-process and written to a per-message JSON file you can grep / back up."
                    .into(),
            ),
        },
    }
}

// ── Telegram /broadcast → many ─────────────────────────────────

fn telegram_cmd_broadcast() -> Recipe {
    Recipe {
        id: "telegram-cmd-broadcast".into(),
        name: "Telegram /broadcast → many".into(),
        description:
            "Sending /broadcast TEXT in your control chat fans the message out to Signal + Discord."
                .into(),
        icon_id: "telegram".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["telegram".into(), "signal".into(), "discord".into(), "broadcast".into()],
        connectors_used: vec![
            "connector-telegram".into(),
            "connector-signal".into(),
            "connector-discord".into(),
        ],
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
                id: "control_chat_id".into(),
                label: "Control chat id (where you type /broadcast)".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Numeric chat id. Lock down — anyone in this chat can broadcast.".into()),
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
                label: "Signal recipient number or group id".into(),
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
                label: "Discord destination channel id".into(),
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
                    connector_name: "connector-signal".into(),
                    config: json!({ "number": "${signal_number}" }),
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
                toml: r#"name = "telegram-cmd-broadcast"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "command_received"

[[conditions]]
type = "FieldEquals"
field = "command"
value = "broadcast"

[[conditions]]
type = "FieldEquals"
field = "chat_id"
value = "${control_chat_id}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "${trigger.args}"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "${discord_channel_id}"
content = "${trigger.args}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Use Telegram as your control plane. `/broadcast Landed safely.` fans the message out to Signal + Discord."
                    .into(),
            ),
        },
    }
}

// ── Bluesky thread scheduler ────────────────────────────────────

fn bluesky_thread_scheduler() -> Recipe {
    Recipe {
        id: "bluesky-thread-scheduler".into(),
        name: "Bluesky Thread Scheduler".into(),
        description:
            "Read scheduled threads from a local queue file and post one Bluesky thread per tick."
                .into(),
        icon_id: "newspaper".into(),
        category: RecipeCategory::Messaging,
        tags: vec!["bluesky".into(), "scheduler".into(), "creator".into()],
        connectors_used: vec!["connector-bluesky".into(), "connector-shell".into()],
        ai_required: false,
        difficulty: Difficulty::Power,
        source: RecipeSource::Builtin,
        inputs: vec![
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
                id: "queue_file".into(),
                label: "Queue file path".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: Some(json!("/var/lib/springtale/bsky-queue.txt")),
                hint: Some(
                    "One post per line. The recipe pops the head line each tick and posts it."
                        .into(),
                ),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-bluesky".into(),
                config: json!({
                    "handle": "${bsky_handle}",
                    "app_password": "${bsky_app_password}"
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "bluesky-thread-scheduler"

[trigger]
type = "Cron"
expression = "*/15 * * * *"

[[actions]]
type = "RunShell"
command = "head -n 1 ${queue_file}"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${last_shell_output}"

[[actions]]
type = "RunShell"
command = "sed -i '1d' ${queue_file}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "Every 15 minutes, the recipe pops the next line from your queue file and posts it to Bluesky. Stop the rule to pause the queue."
                    .into(),
            ),
        },
    }
}
