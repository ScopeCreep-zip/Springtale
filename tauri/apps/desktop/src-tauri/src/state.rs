use std::sync::Arc;

use tokio::sync::Mutex;

use springtale_crypto::vault::store::Vault;

use crate::autolock::AutoLockHandle;

/// Shared application state for Tauri commands.
///
/// The desktop app IS springtaled with a GUI. Same runtime underneath.
/// Wraps RuntimeState (shared with springtaled) plus desktop-specific
/// fields (vault UI, auto-lock timer).
///
/// Commands access shared functionality via `state.runtime` and call
/// `springtale_runtime::operations::*` — same functions springtaled uses.
pub struct AppState {
    /// Shared runtime — store, registry, engine, AI, sentinel, canvas.
    /// Initialized via springtale_runtime::init().
    pub runtime: springtale_runtime::RuntimeState,
    /// Vault — managed via UI (user types passphrase).
    /// Separate from runtime because desktop handles unlock interactively.
    pub vault: Arc<Mutex<Option<Vault>>>,
    /// Auto-lock timer (Rust backend, not JS).
    pub auto_lock: Arc<Mutex<AutoLockHandle>>,
}

impl AppState {
    /// Initialize with the full shared runtime (same boot as springtaled).
    ///
    /// Loads configuration from `springtale.toml` (same as springtaled)
    /// with env var overrides. Connector configs are extracted as raw JSON
    /// values so the factory system can instantiate them.
    pub async fn init() -> Result<Self, anyhow::Error> {
        let mut config = springtale_runtime::RuntimeConfig::default();

        // Load connector configs from springtale.toml if it exists
        let figment = figment::Figment::new()
            .merge(
                <figment::providers::Toml as figment::providers::Format>::file("springtale.toml"),
            )
            .merge(
                figment::providers::Env::prefixed("SPRINGTALE_")
                    .map(|key| key.as_str().replace("__", ".").into()),
            );

        // Extract connector sections as raw JSON values
        let connector_keys = [
            "telegram", "nostr", "irc", "discord", "slack", "signal", "github", "kick",
            "presearch", "bluesky", "http", "filesystem", "shell", "browser",
        ];
        for key in connector_keys {
            if let Ok(val) = figment.extract_inner::<serde_json::Value>(key) {
                config.connector_configs.insert(key.to_string(), val);
            }
        }

        // Extract AI configs if present
        if let Ok(val) = figment.extract_inner::<springtale_ai::OllamaConfig>("ai_ollama") {
            config.ai_ollama = Some(val);
        }
        if let Ok(val) = figment.extract_inner::<springtale_ai::OpenAiConfig>("ai_openai") {
            config.ai_openai = Some(val);
        }
        if let Ok(val) = figment.extract_inner::<springtale_ai::AnthropicConfig>("ai_anthropic") {
            config.ai_anthropic = Some(val);
        }

        let runtime = springtale_runtime::init(&config).await?;

        Ok(Self {
            runtime,
            vault: Arc::new(Mutex::new(None)),
            auto_lock: Arc::new(Mutex::new(AutoLockHandle::new())),
        })
    }
}
