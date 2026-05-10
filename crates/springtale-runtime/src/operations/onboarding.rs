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
// F-conn-1: PlatformForm + FormField now live in `springtale-connector` so
// each connector crate self-registers its onboarding form via
// `ConnectorFactory::onboarding_form()`. The runtime collects them via
// `inventory::iter::<FactoryEntry>` instead of a hardcoded table.
pub use springtale_connector::{FormField, PlatformForm};
use springtale_connector::FactoryEntry;
use springtale_store::StorageBackend;

use super::config::set_config;
use crate::error::OperationError;

/// Summary of a successful `apply_platform` call.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    pub platform: &'static str,
    pub stored_key: String,
    pub fields_stored: Vec<String>,
}

/// List every platform the wizard knows how to configure.
///
/// Iterates compile-time-registered connector factories
/// (`inventory::iter::<FactoryEntry>`) and collects every
/// `Some(onboarding_form)`. Adding a new platform connector requires zero
/// edits here — the connector's own `onboarding_form()` impl gets picked
/// up automatically.
pub fn list_platforms() -> Vec<&'static PlatformForm> {
    inventory::iter::<FactoryEntry>
        .into_iter()
        .filter_map(|e| e.factory.onboarding_form())
        .collect()
}

/// Look up one platform form by its ID. Iterates the same inventory as
/// `list_platforms` so frontends and the apply path stay in sync.
pub fn get_platform(id: &str) -> Option<&'static PlatformForm> {
    inventory::iter::<FactoryEntry>
        .into_iter()
        .find_map(|e| e.factory.onboarding_form().filter(|f| f.id == id))
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

// Platform forms are collected from `inventory::iter::<FactoryEntry>` —
// each connector self-registers its onboarding form via
// `ConnectorFactory::onboarding_form()`. The previous static `PLATFORMS`
// table was deleted for F-conn-1 universality (zero hardcoded names
// outside connector crates per plan §F-conn-1).

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
        for platform in list_platforms() {
            assert!(
                platform.fields.iter().any(|f| f.required),
                "platform {} has no required fields",
                platform.id
            );
        }
    }
}
