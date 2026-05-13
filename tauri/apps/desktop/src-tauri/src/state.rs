use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::{watch, Mutex, RwLock};

use springtale_crypto::vault::store::Vault;

use crate::autolock::AutoLockHandle;
use crate::commands::approval::ApprovalDispatcher;
use crate::runtime_guard::DeferredRuntime;

/// Map of active onboard-stream sessions, keyed by the client-issued
/// session id. The value is the cancel-sender — `send(true)` shuts
/// down the corresponding tokio task within one POLL_INTERVAL.
/// Track D pre-deploy auto-onboard flow.
pub type OnboardSessions = Arc<Mutex<HashMap<String, watch::Sender<bool>>>>;

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
    /// W1.F — Pending approval requests keyed by UUID. The dispatcher
    /// task drops a `oneshot::Sender<bool>` here when it emits an
    /// `approval-required` event; `respond_to_approval` removes the
    /// entry and sends the decision back to the awaiting sentinel.
    pub approval_dispatcher: Arc<ApprovalDispatcher>,
    /// Track D — active pre-deploy onboard-stream cancel senders,
    /// keyed by the session id the frontend mints. `start_onboard_stream`
    /// inserts; `cancel_onboard_stream` and the task's own self-cleanup
    /// remove. Holding it on `AppState` keeps the cancel surface
    /// reachable from any command without threading the map manually.
    pub onboard_sessions: OnboardSessions,
    /// Track E — in-process scheduler + job queue + trigger event loop.
    /// Mirrors what the daemon owns in its own `AppState.scheduler` —
    /// the same `springtale_runtime::EmbeddedScheduler` type drives
    /// both surfaces. Populated by `init_runtime` after the runtime is
    /// ready; `None` while the vault is still locked. New rules
    /// deployed via Tauri commands call `scheduler.schedule(&rule)` so
    /// their cron triggers actually fire in-process.
    pub scheduler: Arc<RwLock<Option<springtale_runtime::EmbeddedScheduler>>>,
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
            approval_dispatcher: Arc::new(ApprovalDispatcher::new()),
            onboard_sessions: Arc::new(Mutex::new(HashMap::new())),
            scheduler: Arc::new(RwLock::new(None)),
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
    scheduler_slot: &Arc<RwLock<Option<springtale_runtime::EmbeddedScheduler>>>,
    encryption_key_hex: Option<String>,
    approval_gate: Option<Arc<dyn springtale_sentinel::ApprovalGate>>,
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

    // F-conn-1 universal-modular-connectors: enumerate connector config
    // keys from the compile-time `inventory::iter::<FactoryEntry>` instead
    // of hardcoding a list. New connectors plug in without editing state.rs.
    for entry in inventory::iter::<springtale_connector::factory::FactoryEntry> {
        let key = entry.factory.config_key();
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

    // Formation command channel — drop the receiver for now; the
    // desktop doesn't run the bot event loop today, so formation
    // commands sent by operations have nowhere to land. (Wiring the
    // bot loop in-process is a follow-up to make the desktop fully
    // self-contained — see CLAUDE.md "Desktop IS springtaled with a
    // GUI".)
    let (formation_cmd_tx, _formation_cmd_rx) =
        tokio::sync::mpsc::channel::<springtale_cooperation::command::FormationCommand>(32);

    let runtime = springtale_runtime::init(&config, formation_cmd_tx, None, approval_gate)
        .await
        .map_err(|e| format!("failed to initialize runtime: {e}"))?;

    // Track E — bring up the in-process scheduler + job queue + trigger
    // event loop. Same `bootstrap_embedded` the daemon uses, so cron
    // expressions registered through the desktop UI actually tick.
    // Heartbeat is off by default for the desktop — heartbeat fires
    // a `SystemEvent("heartbeat")` trigger which only a few rules
    // listen for, and the daemon's default `heartbeat_interval_secs`
    // is read from `springtaled.toml` (absent here).
    let handle = springtale_runtime::bootstrap_embedded(&runtime, 0)
        .await
        .map_err(|e| format!("failed to bootstrap scheduler: {e}"))?;

    *deferred.write().await = Some(runtime);
    *scheduler_slot.write().await = Some(handle.scheduler);
    Ok(())
}

/// W1.F — build a `ChannelApprovalGate` wired to a background
/// dispatcher that emits `approval-required` events to the frontend.
/// Pass the resulting gate to [`init_runtime`] so the sentinel
/// prompts the user instead of silently denying destructive actions.
///
/// 60-second timeout: if a survivor steps away mid-prompt we deny by
/// default, but the window is long enough that an authorised user
/// reading the dialog won't be auto-denied while they read.
pub fn build_approval_gate(
    app: AppHandle,
    dispatcher: Arc<ApprovalDispatcher>,
) -> Arc<dyn springtale_sentinel::ApprovalGate> {
    crate::commands::approval::install(app, dispatcher, Duration::from_secs(60))
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
