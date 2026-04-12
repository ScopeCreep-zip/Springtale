use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use springtale_crypto::vault::store::Vault;

use crate::autolock::AutoLockHandle;
use crate::runtime_guard::DeferredRuntime;

/// Shared application state for Tauri commands.
///
/// The desktop app IS springtaled with a GUI. Same runtime underneath.
/// Wraps a deferred `RuntimeState` (populated after vault unlock) plus
/// desktop-specific fields (vault UI, auto-lock timer).
///
/// Pattern: tauri-plugin-stronghold — the vault entry is absent until
/// `initialize` is called. Commands that need it call `require_runtime()`
/// and get a clean `"Vault is locked"` error if it's not ready.
pub struct AppState {
    /// Shared runtime — None until vault is unlocked and the DB key is
    /// derived. Populated by `unlock_vault` / `create_vault` commands.
    pub runtime: DeferredRuntime,
    /// Vault — managed via UI (user types passphrase).
    pub vault: Arc<Mutex<Option<Vault>>>,
    /// Auto-lock timer (Rust backend, not JS).
    pub auto_lock: Arc<Mutex<AutoLockHandle>>,
}

impl AppState {
    /// Create the app shell — instant, no DB access, no passphrase.
    ///
    /// The runtime is `None` until the user unlocks the vault via the
    /// frontend overlay. This lets the Tauri window open immediately
    /// and show the passphrase prompt without blocking on DB access.
    pub fn shell() -> Self {
        Self {
            runtime: Arc::new(RwLock::new(None)),
            vault: Arc::new(Mutex::new(None)),
            auto_lock: Arc::new(Mutex::new(AutoLockHandle::new())),
        }
    }
}

/// Initialize the runtime with the derived encryption key.
///
/// Called from `unlock_vault` / `create_vault` after the passphrase
/// is known. Loads config from `springtale.toml`, extracts connector
/// and AI configs, then calls `springtale_runtime::init()`. The result
/// is stored in the deferred runtime slot.
///
/// This is the same boot sequence as `AppState::init()` was before,
/// just deferred until after vault unlock instead of running at startup.
pub async fn init_runtime(
    deferred: &DeferredRuntime,
    encryption_key_hex: Option<String>,
) -> Result<(), String> {
    let mut config = springtale_runtime::RuntimeConfig::default();
    config.store.encryption_key_hex = encryption_key_hex;

    let figment = figment::Figment::new()
        .merge(
            <figment::providers::Toml as figment::providers::Format>::file("springtale.toml"),
        )
        .merge(
            figment::providers::Env::prefixed("SPRINGTALE_")
                .map(|key| key.as_str().replace("__", ".").into()),
        );

    let connector_keys = [
        "telegram", "nostr", "irc", "discord", "slack", "signal", "github", "kick",
        "presearch", "bluesky", "http", "filesystem", "shell", "browser",
    ];
    for key in connector_keys {
        if let Ok(val) = figment.extract_inner::<serde_json::Value>(key) {
            config.connector_configs.insert(key.to_string(), val);
        }
    }

    if let Ok(val) = figment.extract_inner::<springtale_ai::OllamaConfig>("ai_ollama") {
        config.ai_ollama = Some(val);
    }
    if let Ok(val) = figment.extract_inner::<springtale_ai::OpenAiConfig>("ai_openai") {
        config.ai_openai = Some(val);
    }
    if let Ok(val) = figment.extract_inner::<springtale_ai::AnthropicConfig>("ai_anthropic") {
        config.ai_anthropic = Some(val);
    }

    let runtime = springtale_runtime::init(&config)
        .await
        .map_err(|e| format!("failed to initialize runtime: {e}"))?;

    *deferred.write().await = Some(runtime);
    Ok(())
}

/// Check if the existing database requires an encryption key to open.
///
/// Returns `true` if the DB file exists AND its header is NOT the
/// standard SQLite magic bytes (`SQLite format 3\0`). An encrypted DB
/// has random bytes in the header instead.
pub fn detect_encryption_needed() -> bool {
    let db_path = springtale_store::paths::default_db_path();
    if !db_path.exists() {
        return false;
    }
    match std::fs::read(&db_path) {
        Ok(bytes) if bytes.len() >= 16 => !bytes.starts_with(b"SQLite format 3\0"),
        _ => false,
    }
}
