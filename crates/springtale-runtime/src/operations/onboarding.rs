//! Onboarding — backend-driven first-run wizard.
//!
//! Platform forms (what fields to collect) and persistence (how to store
//! the answers) live here so every frontend that runs the wizard speaks
//! the same language. The CLI's `springtale init` and the Tauri desktop
//! first-run screen both iterate [`list_platforms`] and call
//! [`apply_platform`] with the user's answers.
//!
//! # Security guarantees
//!
//! - **Secrets never land in TOML.** [`apply_platform`] writes every
//!   answer (including bot tokens) into the `config_store` table, which
//!   is inside the SQLite database the daemon encrypts at rest with
//!   `springtale_crypto::token::derive_db_encryption_key`. The CLI's old
//!   behaviour (`init.rs` v1) appended bot tokens to user-editable
//!   `springtale.toml` — exactly the kind of file that ends up in
//!   backups, bug reports, and screenshots. This module exists to make
//!   that foot-gun unreachable.
//! - **No network calls.** Wizards just persist config; the daemon
//!   handles all outbound connections when it loads the connector.
//! - **Input validation.** Platform names are checked against the static
//!   list; unknown fields in `answers` are rejected to prevent a caller
//!   from smuggling arbitrary JSON into the config store.

use std::collections::BTreeMap;

use serde::Serialize;
use springtale_store::StorageBackend;

use super::config::set_config;
use crate::error::OperationError;

/// A single field the user must fill in for a platform.
#[derive(Debug, Clone, Serialize)]
pub struct FormField {
    /// Stable machine key used as the JSON property name.
    pub name: &'static str,
    /// Human label shown by the frontend.
    pub label: &'static str,
    /// Short hint/help text.
    pub description: &'static str,
    /// Frontend should mask input (password prompt, hidden field).
    pub secret: bool,
    /// Optional default value the user can accept without typing.
    pub default: Option<&'static str>,
    pub required: bool,
    /// Regex pattern the answer must match (OWASP ASVS §5.1.4).
    /// None = no format restriction beyond non-empty.
    pub validation: Option<&'static str>,
}

/// One platform the onboarding wizard knows how to set up.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformForm {
    /// Stable ID used in `apply_platform` calls.
    pub id: &'static str,
    /// Internal config key (also the connector's `config_key`).
    pub config_key: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    pub description: &'static str,
    pub setup_help: &'static str,
    pub fields: &'static [FormField],
}

impl PlatformForm {
    pub fn field(&self, name: &str) -> Option<&'static FormField> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// Summary of a successful `apply_platform` call.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    pub platform: &'static str,
    pub stored_key: String,
    pub fields_stored: Vec<String>,
}

/// List every platform the wizard knows how to configure.
pub fn list_platforms() -> &'static [PlatformForm] {
    PLATFORMS
}

/// Look up one platform form by its ID.
pub fn get_platform(id: &str) -> Option<&'static PlatformForm> {
    PLATFORMS.iter().find(|p| p.id == id)
}

/// Persist a completed wizard answer set as a connector config.
///
/// The `answers` map must only contain keys declared in the platform's
/// `fields` — extra keys are rejected. Missing required fields are
/// rejected. Values are stored as JSON under `connector:{config_key}`
/// in the config_store table.
pub async fn apply_platform(
    store: &dyn StorageBackend,
    platform_id: &str,
    answers: BTreeMap<String, String>,
) -> Result<ApplyReport, OperationError> {
    // HA pattern: onboarding endpoints auto-lock after first success.
    if let Ok(Some(val)) = store.get_config("onboarded").await
        && val.trim_matches('"') == "true"
    {
        return Err(OperationError::Validation(
            "onboarding already completed — use the dashboard or API to add more connectors"
                .into(),
        ));
    }

    let platform = get_platform(platform_id).ok_or_else(|| {
        OperationError::Validation(format!("unknown onboarding platform: {platform_id}"))
    })?;

    for key in answers.keys() {
        if platform.field(key).is_none() {
            return Err(OperationError::Validation(format!(
                "unknown field '{key}' for platform '{platform_id}'"
            )));
        }
    }

    // Build the config object: every declared field, with defaults filled
    // in if the user didn't supply one. Reject empty required fields.
    let mut config = serde_json::Map::new();
    let mut fields_stored = Vec::with_capacity(platform.fields.len());
    for field in platform.fields {
        let value = match answers.get(field.name) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => match field.default {
                Some(default) => default.to_owned(),
                None => {
                    if field.required {
                        return Err(OperationError::Validation(format!(
                            "missing required field '{}' for platform '{platform_id}'",
                            field.name
                        )));
                    }
                    continue;
                }
            },
        };
        // Validate format if the field declares a pattern.
        if let Some(pattern) = field.validation {
            let re = regex::Regex::new(pattern).map_err(|e| {
                OperationError::Validation(format!(
                    "internal: bad validation pattern for '{}': {e}",
                    field.name
                ))
            })?;
            if !re.is_match(&value) {
                return Err(OperationError::Validation(format!(
                    "field '{}' does not match expected format",
                    field.name
                )));
            }
        }
        config.insert(field.name.to_owned(), serde_json::Value::String(value));
        fields_stored.push(field.name.to_owned());
    }

    let stored_key = format!("connector:{}", platform.config_key);
    set_config(store, &stored_key, serde_json::Value::Object(config)).await?;

    tracing::info!(
        platform = platform_id,
        stored_key = %stored_key,
        fields = ?fields_stored,
        "onboarding applied"
    );

    // Lock onboarding after first successful apply (HA pattern).
    let _ = store.set_config("onboarded", "\"true\"").await;

    Ok(ApplyReport {
        platform: platform.id,
        stored_key,
        fields_stored,
    })
}

// ---------- Static platform table ----------
//
// Each platform's `config_key` must match the connector factory's
// `config_key()` — this is what drives `init_registry()` in init.rs to
// actually load the connector at daemon boot.

static PLATFORMS: &[PlatformForm] = &[
    PlatformForm {
        id: "telegram",
        config_key: "telegram",
        label: "Telegram",
        description: "Connect a Telegram bot via polling",
        setup_help: "Create a bot with @BotFather in Telegram. Copy the HTTP API token it returns.",
        fields: &[
            FormField {
                name: "bot_token",
                label: "Bot token",
                description: "Telegram Bot API token from @BotFather",
                secret: true,
                default: None,
                required: true,
                validation: Some(r"^\d+:[A-Za-z0-9_-]+$"),
            },
            FormField {
                name: "update_mode",
                label: "Update mode",
                description: "polling (no public URL needed) or webhook",
                secret: false,
                default: Some("polling"),
                required: false,
                validation: Some(r"^(polling|webhook)$"),
            },
        ],
    },
    PlatformForm {
        id: "discord",
        config_key: "discord",
        label: "Discord",
        description: "Connect a Discord bot",
        setup_help: "Create an application at discord.com/developers, then copy the Bot token and Application ID.",
        fields: &[
            FormField {
                name: "bot_token",
                label: "Bot token",
                description: "Discord bot token",
                secret: true,
                default: None,
                required: true,
                validation: None,
            },
            FormField {
                name: "application_id",
                label: "Application ID",
                description: "Discord application (client) ID",
                secret: false,
                default: None,
                required: true,
                validation: Some(r"^\d{17,20}$"),
            },
        ],
    },
    PlatformForm {
        id: "slack",
        config_key: "slack",
        label: "Slack",
        description: "Connect a Slack app (Socket Mode)",
        setup_help: "Create an app at api.slack.com/apps, enable Socket Mode, generate a Bot token (xoxb-) and App token (xapp-).",
        fields: &[
            FormField {
                name: "bot_token",
                label: "Bot token",
                description: "xoxb-... bot user OAuth token",
                secret: true,
                default: None,
                required: true,
                validation: Some(r"^xoxb-"),
            },
            FormField {
                name: "app_token",
                label: "App token",
                description: "xapp-... app-level token with connections:write",
                secret: true,
                default: None,
                required: true,
                validation: Some(r"^xapp-"),
            },
        ],
    },
    PlatformForm {
        id: "signal",
        config_key: "signal",
        label: "Signal",
        description: "Connect via a signal-cli daemon",
        setup_help: "Install signal-cli and run it in daemon mode. See https://github.com/AsamK/signal-cli.",
        fields: &[
            FormField {
                name: "daemon_url",
                label: "Daemon URL",
                description: "Address where signal-cli is listening",
                secret: false,
                default: Some("http://localhost:8080"),
                required: true,
                validation: Some(r"^https?://"),
            },
            FormField {
                name: "account_id",
                label: "Account ID",
                description: "Phone number / account identifier registered with signal-cli",
                secret: false,
                default: Some("default"),
                required: true,
                validation: None,
            },
        ],
    },
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_store::backend::sqlite::SqliteBackend;
    use std::sync::Arc;

    fn store() -> Arc<dyn StorageBackend> {
        Arc::new(SqliteBackend::open_in_memory().unwrap())
    }

    #[tokio::test]
    async fn apply_platform_stores_connector_config() {
        let store = store();
        let mut answers = BTreeMap::new();
        // Must match Telegram bot_token regex: ^\d+:[A-Za-z0-9_-]+$
        answers.insert("bot_token".into(), "123456789:ABCdefGHI-jkl_MNO".into());
        let report = apply_platform(&*store, "telegram", answers).await.unwrap();
        assert_eq!(report.platform, "telegram");
        assert_eq!(report.stored_key, "connector:telegram");
        assert!(report.fields_stored.contains(&"bot_token".to_owned()));
        assert!(report.fields_stored.contains(&"update_mode".to_owned()));

        let stored = store.get_config("connector:telegram").await.unwrap().unwrap();
        let json: serde_json::Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(json["bot_token"], "123456789:ABCdefGHI-jkl_MNO");
        assert_eq!(json["update_mode"], "polling");
    }

    #[tokio::test]
    async fn apply_platform_rejects_unknown_field() {
        let store = store();
        let mut answers = BTreeMap::new();
        answers.insert("bot_token".into(), "111:abc".into());
        answers.insert("hacker_payload".into(), "!!".into());
        let err = apply_platform(&*store, "telegram", answers).await.unwrap_err();
        assert!(matches!(err, OperationError::Validation(_)));
    }

    #[tokio::test]
    async fn apply_platform_rejects_unknown_platform() {
        let store = store();
        let err = apply_platform(&*store, "myspace", BTreeMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, OperationError::Validation(_)));
    }

    #[tokio::test]
    async fn apply_platform_rejects_missing_required_field() {
        let store = store();
        let err = apply_platform(&*store, "discord", BTreeMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, OperationError::Validation(_)));
    }

    #[test]
    fn every_platform_has_at_least_one_required_field() {
        for platform in PLATFORMS {
            assert!(
                platform.fields.iter().any(|f| f.required),
                "platform {} has no required fields",
                platform.id
            );
        }
    }
}
