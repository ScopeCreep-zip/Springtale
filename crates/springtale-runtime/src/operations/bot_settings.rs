//! Bot settings — persona, context window and tool policy as *settings*,
//! not as boot-time TOML the user has to edit and restart for.
//!
//! Plan 6.3 / finding 105. These three knobs used to live in the `[bot]`
//! section of `springtale.toml`, which meant "change your bot's name"
//! was "edit a file, restart the daemon". The product model forbids
//! that. They now live in the config store under [`KEY`], are cached in
//! `RuntimeState::bot_settings` behind an `ArcSwap` for lock-free reads
//! on the chat hot path, and are edited over `GET|PUT /bot/settings`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use springtale_store::StorageBackend;

use crate::error::OperationError;
use crate::operations::config::{get_config, set_config};
use crate::state::RuntimeState;

/// Config-store key holding the serialized [`BotSettings`].
pub const KEY: &str = "bot:settings";

/// Bot persona — display name, tone hint, command prefix.
///
/// Moved here from `springtale-bot` (plan 6.3): the bot depends on the
/// runtime, not the reverse, so the settings type the runtime owns and
/// hands out has to live on this side of the dependency edge.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, utoipa::ToSchema)]
pub struct BotPersona {
    /// Bot display name. Default: "Springtale".
    #[serde(default = "default_name")]
    pub name: String,
    /// Response tone hint. Default: "neutral".
    #[serde(default = "default_tone")]
    pub tone: String,
    /// Command prefix character. Default: '/'.
    #[serde(default = "default_prefix")]
    pub prefix: char,
}

fn default_name() -> String {
    "Springtale".to_owned()
}

fn default_tone() -> String {
    "neutral".to_owned()
}

fn default_prefix() -> char {
    '/'
}

fn default_context_window() -> usize {
    50
}

/// Five minutes, matching the `vault_timeout_secs` this setting replaced.
fn default_auto_lock_secs() -> u64 {
    300
}

/// Session idle timeout: 30 minutes (plan 6.6).
fn default_session_idle_secs() -> u64 {
    1_800
}

/// Session absolute lifetime: 12 hours (plan 6.6). OWASP: "the absolute
/// timeout limits the maximum amount of time a session can be active"
/// regardless of activity.
fn default_session_absolute_secs() -> u64 {
    43_200
}

impl Default for BotPersona {
    fn default() -> Self {
        Self {
            name: default_name(),
            tone: default_tone(),
            prefix: default_prefix(),
        }
    }
}

/// The user-editable bot settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, utoipa::ToSchema)]
pub struct BotSettings {
    /// Persona (name, tone, command prefix).
    #[serde(default)]
    pub persona: BotPersona,
    /// Conversation context window size. Default: 50.
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    /// Which connector actions the AI may call as tools. Default: empty
    /// allow-list — the same default-mode posture as before (read-only
    /// actions only; see `springtale_ai::ToolPolicy`).
    #[serde(default)]
    #[schema(value_type = Object)]
    pub tool_policy: springtale_ai::ToolPolicy,
    /// Idle seconds before the daemon locks itself: drops the runtime,
    /// closes the database and zeroizes the vault key (plan 6.10).
    /// `0` disables auto-lock. Default: 300.
    ///
    /// Moved here from `springtale_bot::BotConfig::vault_timeout_secs`,
    /// which nothing read and which could only be changed by editing a
    /// TOML file and restarting — exactly what the product model
    /// forbids of a setting.
    #[serde(default = "default_auto_lock_secs")]
    pub auto_lock_secs: u64,
    /// Management-API session idle timeout, seconds. A session with no
    /// accepted request inside this window is dropped. Default 1800.
    #[serde(default = "default_session_idle_secs")]
    pub session_idle_secs: u64,
    /// Management-API session absolute lifetime, seconds. A session is
    /// dropped this long after login however active it is, so a stolen
    /// token has a bounded life. Default 43200 (12 h).
    #[serde(default = "default_session_absolute_secs")]
    pub session_absolute_secs: u64,
}

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            persona: BotPersona::default(),
            context_window: default_context_window(),
            tool_policy: springtale_ai::ToolPolicy::default(),
            auto_lock_secs: default_auto_lock_secs(),
            session_idle_secs: default_session_idle_secs(),
            session_absolute_secs: default_session_absolute_secs(),
        }
    }
}

/// Read the stored settings. A missing key yields the defaults — a fresh
/// install is a working install, no seeding step required.
pub async fn get(store: &dyn StorageBackend) -> Result<BotSettings, OperationError> {
    let value = get_config(store, KEY).await?;
    if value.is_null() {
        return Ok(BotSettings::default());
    }
    serde_json::from_value(value)
        .map_err(|e| OperationError::Validation(format!("invalid bot settings: {e}")))
}

/// Persist settings and publish them to the live runtime.
///
/// Every literal (non-glob) allow-list entry is validated against the
/// connector registry first: a typo'd tool name would otherwise silently
/// hand the model an empty tool list, which reads as "the AI is broken"
/// rather than "you misspelled a setting". Glob patterns are accepted as
/// written — they legitimately describe actions from connectors that are
/// not installed yet.
pub async fn set(state: &RuntimeState, settings: BotSettings) -> Result<(), OperationError> {
    if settings.context_window == 0 {
        return Err(OperationError::Validation(
            "context_window must be at least 1".to_owned(),
        ));
    }
    if settings.session_idle_secs < 60 {
        return Err(OperationError::Validation(
            "session_idle_secs must be at least 60".to_owned(),
        ));
    }
    if settings.session_absolute_secs < settings.session_idle_secs {
        return Err(OperationError::Validation(
            "session_absolute_secs must be at least session_idle_secs".to_owned(),
        ));
    }
    {
        let registry = state.registry.read().await;
        for allowed in &settings.tool_policy.allow {
            if allowed.contains('*') {
                continue;
            }
            if !registry.has_action(allowed) {
                return Err(OperationError::Validation(format!(
                    "unknown tool {allowed}"
                )));
            }
        }
    }

    let value = serde_json::to_value(&settings)
        .map_err(|e| OperationError::Validation(format!("failed to serialize settings: {e}")))?;
    set_config(&*state.store, KEY, value).await?;
    state.bot_settings.store(Arc::new(settings));
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Boot a runtime over an ephemeral in-memory store.
    async fn boot() -> RuntimeState {
        let config = crate::RuntimeConfig {
            store: crate::StoreConfig {
                ephemeral: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let (formation_cmd_tx, _rx) = tokio::sync::mpsc::channel(16);
        crate::init(&config, formation_cmd_tx, None, None)
            .await
            .expect("runtime init")
    }

    #[tokio::test]
    async fn test_set_unknown_tool_rejected() {
        let state = boot().await;
        let settings = BotSettings {
            tool_policy: springtale_ai::ToolPolicy {
                allow: vec!["connector-nope__do_thing".to_owned()],
                ..Default::default()
            },
            ..Default::default()
        };

        let err = set(&state, settings).await.expect_err("must reject");
        assert!(
            matches!(&err, OperationError::Validation(m) if m.contains("unknown tool")),
            "expected validation error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_get_returns_defaults_when_absent() {
        let state = boot().await;
        let settings = get(&*state.store).await.expect("defaults");
        assert_eq!(settings.persona.prefix, '/');
        assert_eq!(settings.context_window, 50);
        assert_eq!(settings.auto_lock_secs, 300);
    }

    #[tokio::test]
    async fn test_auto_lock_secs_round_trips() {
        let state = boot().await;
        let settings = BotSettings {
            auto_lock_secs: 30,
            ..Default::default()
        };
        set(&state, settings).await.expect("store settings");
        let read_back = get(&*state.store).await.expect("read back");
        assert_eq!(read_back.auto_lock_secs, 30);
        assert_eq!(state.bot_settings.load().auto_lock_secs, 30);
    }
}
