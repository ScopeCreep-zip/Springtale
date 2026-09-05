//! AI Assistant recipes — LLM provider configuration + AI-augmented
//! conversational flows. Connectors: any messaging connector + the
//! built-in AI adapter (Ollama / OpenAI / Anthropic).

use serde_json::json;

use super::super::types::{
    AiConfigStep, ConnectorConfigStep, Difficulty, FieldKind, FieldVisibility, InputField, Recipe,
    RecipeBlueprint, RecipeCategory, RecipeSource, RuleStep, SelectOption,
};

pub fn all() -> Vec<Recipe> {
    vec![
        llm_assistant(),
        ai_translate_incoming(),
        ai_wellness_check_in(),
        ai_cw_classifier(),
    ]
}

// ── LLM provider config (existing) ─────────────────────────────

fn llm_assistant() -> Recipe {
    Recipe {
        id: "llm-assistant".into(),
        name: "LLM Assistant".into(),
        description: "Chat with a local or remote LLM. Works fully offline with Ollama.".into(),
        icon_id: "robot".into(),
        category: RecipeCategory::AiAssistant,
        tags: vec!["ai".into(), "chat".into(), "local-first".into()],
        connectors_used: vec![],
        ai_required: true,
        difficulty: Difficulty::Quick,
        source: RecipeSource::Builtin,
        inputs: vec![
            InputField {
                id: "provider".into(),
                label: "AI provider".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption {
                            value: "ollama".into(),
                            label: "Ollama (local, no network)".into(),
                        },
                        SelectOption {
                            value: "openai".into(),
                            label: "OpenAI".into(),
                        },
                        SelectOption {
                            value: "anthropic".into(),
                            label: "Anthropic".into(),
                        },
                    ],
                },
                visibility: FieldVisibility::Required,
                default: Some(json!("ollama")),
                hint: Some("Ollama runs locally and never sends data to a server.".into()),
            },
            InputField {
                id: "model".into(),
                label: "Model".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("llama3")),
                hint: Some("e.g. llama3, gpt-4o-mini, claude-sonnet-4-6".into()),
            },
            InputField {
                id: "api_key".into(),
                label: "API key".into(),
                kind: FieldKind::Secret,
                visibility: FieldVisibility::Optional,
                default: None,
                hint: Some("Only needed for OpenAI / Anthropic; ignored for Ollama.".into()),
            },
            InputField {
                id: "base_url".into(),
                label: "API base URL".into(),
                kind: FieldKind::Url,
                visibility: FieldVisibility::Advanced,
                default: Some(json!("http://localhost:11434")),
                hint: Some("Defaults work for stock Ollama installs.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![],
            rules: vec![],
            ai_config: Some(AiConfigStep {
                target: crate::operations::config::AiTarget::Colony,
                config: json!({
                    "adapter": "${provider}",
                    "base_url": "${base_url}",
                    "model": "${model}",
                    "api_key": "${api_key}"
                }),
            }),
            summary: Some(
                "Configures an AI provider so any bot can use it. Ollama runs locally.".into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── AI translate incoming ─────────────────────────────────────

fn ai_translate_incoming() -> Recipe {
    Recipe {
        id: "ai-translate-incoming".into(),
        name: "AI Translate Incoming Messages".into(),
        description:
            "When a Telegram message arrives, send back a translation into your target language."
                .into(),
        icon_id: "robot".into(),
        category: RecipeCategory::AiAssistant,
        tags: vec!["telegram".into(), "ai".into(), "translate".into()],
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
                id: "target_lang".into(),
                label: "Target language".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("English")),
                hint: Some("e.g. English, Spanish, Arabic. The AI handles detection.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-telegram".into(),
                config: json!({ "bot_token": "${bot_token}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "ai-translate-incoming"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message"

[[actions]]
type = "AiComplete"
prompt = "Detect the language of this message and translate it to ${target_lang} if it isn't already. Reply ONLY with the translation, no preamble.\n\n${trigger.text}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat_id}"
text = "${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "AI translates incoming Telegram messages into your target language and replies inline. Useful for multilingual community chats."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── AI wellness check-in ──────────────────────────────────────

fn ai_wellness_check_in() -> Recipe {
    Recipe {
        id: "ai-wellness-check-in".into(),
        name: "Daily Wellness Check-In".into(),
        description: "Evening one-line check-in via Signal, AI-drafted from your prompt.".into(),
        icon_id: "shield".into(),
        category: RecipeCategory::AiAssistant,
        tags: vec!["signal".into(), "ai".into(), "wellness".into(), "daily".into()],
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
                label: "Signal recipient".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Required,
                default: None,
                hint: Some("Usually your own number.".into()),
            },
            InputField {
                id: "prompt_seed".into(),
                label: "Prompt seed".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!(
                    "Write one gentle, specific question I can answer to check in with myself tonight. Vary it day-to-day."
                )),
                hint: Some("How the AI shapes the daily question.".into()),
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-signal".into(),
                config: json!({ "number": "${signal_number}" }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "ai-wellness-check-in"

[trigger]
type = "Cron"
expression = "0 19 * * *"

[[actions]]
type = "AiComplete"
prompt = "${prompt_seed}"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
to = "${signal_recipient}"
text = "🌙 ${last_ai_output}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "7pm every day: the LLM drafts one short, varying check-in question and Signal delivers it. Off-by-default; deliberately small."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}

// ── AI content-warning classifier ─────────────────────────────

fn ai_cw_classifier() -> Recipe {
    Recipe {
        id: "ai-cw-classifier".into(),
        name: "AI Content-Warning Classifier".into(),
        description:
            "When a posted message clears a threat threshold, the bot reacts with a CW emoji."
                .into(),
        icon_id: "shield".into(),
        category: RecipeCategory::AiAssistant,
        tags: vec!["discord".into(), "ai".into(), "moderation".into(), "cw".into()],
        connectors_used: vec!["connector-discord".into()],
        ai_required: true,
        difficulty: Difficulty::Power,
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
                id: "threshold".into(),
                label: "Sensitivity".into(),
                kind: FieldKind::Select {
                    options: vec![
                        SelectOption { value: "high".into(), label: "High (only severe slurs)".into() },
                        SelectOption { value: "medium".into(), label: "Medium (slurs + targeted harassment)".into() },
                        SelectOption { value: "low".into(), label: "Low (also flag heavy topics)".into() },
                    ],
                },
                visibility: FieldVisibility::Optional,
                default: Some(json!("medium")),
                hint: Some("How aggressively the AI flags content.".into()),
            },
            InputField {
                id: "cw_emoji".into(),
                label: "CW reaction emoji".into(),
                kind: FieldKind::Text,
                visibility: FieldVisibility::Optional,
                default: Some(json!("⚠️")),
                hint: None,
            },
        ],
        blueprint: RecipeBlueprint {
            connector_configs: vec![ConnectorConfigStep {
                connector_name: "connector-discord".into(),
                config: json!({
                    "bot_token": "${bot_token}",
                    "application_id": "${application_id}",
                    "enable_message_content": true,
                    "enable_reactions": true
                }),
            }],
            rules: vec![RuleStep {
                toml: r#"name = "ai-cw-classifier"

[trigger]
type = "ConnectorEvent"
connector = "connector-discord"
event = "message_received"

[[actions]]
type = "AiComplete"
prompt = "You are a content-warning classifier with ${threshold} sensitivity. Reply with exactly the string `YES` if the message contains slurs, severe harassment, doxxing, or graphic content that warrants a content warning. Reply with `NO` otherwise.\n\nMessage:\n${trigger.content}"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "add_reaction"

[actions.params]
channel_id = "${trigger.channel_id}"
message_id = "${trigger.message_id}"
emoji = "${cw_emoji}"
"#
                .into(),
            }],
            ai_config: None,
            summary: Some(
                "AI reads each posted message and (when the classifier says YES) adds a CW emoji reaction. Heads-up for readers without surfacing or quoting the offending content."
                    .into(),
            ),
            derived_inputs: vec![],
        },
    }
}
