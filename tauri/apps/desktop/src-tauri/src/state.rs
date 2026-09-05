use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::{Mutex, RwLock, watch};

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
    /// ConnectorEvent subscription registry — the desktop equivalent of
    /// the daemon's `AppState.trigger_registry`. Populated by
    /// `init_runtime` after `wire_connector_events` attaches handlers
    /// for all enabled ConnectorEvent rules at boot. Rule
    /// create/delete/toggle/update commands call `attach_rule`/
    /// `detach_rule` so messaging bots (Telegram/Discord/Nostr reply,
    /// echo, …) actually fire on desktop, not just on the daemon.
    pub trigger_registry: Arc<RwLock<Option<springtale_runtime::TriggerRegistry>>>,
    /// In-app chat ingress to the embedded bot event loop. `None` until the
    /// vault is unlocked and `init_runtime` builds the bot; the
    /// `send_chat_message` command pushes `IncomingMessage`s here and the
    /// bot's replies come back as `chat-message` Tauri events.
    pub bot_msg_tx: Arc<RwLock<Option<tokio::sync::mpsc::Sender<springtale_bot::IncomingMessage>>>>,
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
            trigger_registry: Arc::new(RwLock::new(None)),
            bot_msg_tx: Arc::new(RwLock::new(None)),
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
    app: AppHandle,
    deferred: &DeferredRuntime,
    scheduler_slot: &Arc<RwLock<Option<springtale_runtime::EmbeddedScheduler>>>,
    trigger_registry_slot: &Arc<RwLock<Option<springtale_runtime::TriggerRegistry>>>,
    bot_slot: &Arc<RwLock<Option<tokio::sync::mpsc::Sender<springtale_bot::IncomingMessage>>>>,
    encryption_key_hex: Option<String>,
    approval_gate: Option<Arc<dyn springtale_sentinel::ApprovalGate>>,
) -> Result<(), String> {
    let mut config = springtale_runtime::RuntimeConfig::default();
    config.store.encryption_key_hex = encryption_key_hex;

    let figment = figment::Figment::new()
        .merge(<figment::providers::Toml as figment::providers::Format>::file("springtale.toml"))
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

    // Formation command channel — the receiver now feeds the in-process
    // bot event loop (the desktop is "springtaled with a GUI", so it runs
    // the same bot the daemon does).
    let (formation_cmd_tx, formation_cmd_rx) =
        tokio::sync::mpsc::channel::<springtale_cooperation::command::FormationCommand>(32);

    let runtime = springtale_runtime::init(&config, formation_cmd_tx, None, approval_gate)
        .await
        .map_err(|e| format!("failed to initialize runtime: {e}"))?;

    // Plan 6.7 — forward the runtime's event stream to the webview as
    // `event-fired`, the Tauri event the desktop provider's
    // `subscribeToEvents` already listens on. This is the desktop
    // counterpart of the daemon's `GET /events/stream` SSE: without it
    // the shared dashboard state never sees `approval_required` and the
    // pending-approvals panel would only refresh on launch / resolve.
    {
        use tauri::Emitter;
        let app = app.clone();
        let mut rx = runtime.event_tx.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(entry) => {
                        if let Err(e) = app.emit("event-fired", &entry) {
                            tracing::warn!(error = %e, "event-fired emit failed");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Track E — bring up the in-process scheduler + job queue + trigger
    // event loop. Same `bootstrap_embedded` the daemon uses, so cron
    // expressions registered through the desktop UI actually tick.
    // Heartbeat is ON (60s) so `SystemEvent("heartbeat")` rules —
    // periodic safety checks like tor-circuit-rotate-reminder — fire on
    // desktop exactly as they do on the daemon. Previously this was 0
    // (disabled), so every SystemEvent rule was dead on desktop.
    const DESKTOP_HEARTBEAT_SECS: u64 = 60;
    let handle = springtale_runtime::bootstrap_embedded(&runtime, DESKTOP_HEARTBEAT_SECS)
        .await
        .map_err(|e| format!("failed to bootstrap scheduler: {e}"))?;

    // ConnectorEvent handlers are wired inside `bootstrap_embedded`
    // (shared with the daemon), which publishes the registry on
    // `RuntimeState`. Mirror it onto `AppState` so rule CRUD + recipe
    // deploy commands attach/detach through the same instance. Without
    // this, entire messaging categories never fired on desktop.
    if let Some(registry) = runtime.trigger_registry.get().cloned() {
        *trigger_registry_slot.write().await = Some(registry);
    }

    // ── In-process bot event loop (in-app chat) ──
    // Build the same `Bot` the daemon builds, wired so the `send_chat_message`
    // command can inject `IncomingMessage`s and the bot's replies come back as
    // `chat-message` Tauri events. We have no chat-platform gateways here
    // (Telegram/etc. live in the daemon), so the ONLY ingress is in-app chat
    // and the only egress we surface is the `in-app` connector.
    let bot_tx = build_in_process_bot(app, &runtime, handle.scheduler.clone(), formation_cmd_rx)
        .await
        .map_err(|e| format!("failed to start bot: {e}"))?;
    *bot_slot.write().await = Some(bot_tx);

    *deferred.write().await = Some(runtime);
    *scheduler_slot.write().await = Some(handle.scheduler);
    Ok(())
}

/// Reply pushed to the desktop chat panel — payload of the `chat-message`
/// Tauri event. Mirrors the dashboard's `ChatStreamMessage`.
#[derive(Clone, serde::Serialize)]
struct ChatEvent {
    session: String,
    text: String,
}

/// Build the embedded `Bot`, spawn its event loop and a response dispatcher
/// that emits `chat-message` events for `in-app` replies. Returns the chat
/// ingress sender to store on `AppState`.
async fn build_in_process_bot(
    app: AppHandle,
    runtime: &springtale_runtime::RuntimeState,
    scheduler: springtale_runtime::EmbeddedScheduler,
    formation_cmd_rx: tokio::sync::mpsc::Receiver<
        springtale_cooperation::command::FormationCommand,
    >,
) -> Result<tokio::sync::mpsc::Sender<springtale_bot::IncomingMessage>, String> {
    use tauri::Emitter;

    let (bot_msg_tx, bot_msg_rx) =
        tokio::sync::mpsc::channel::<springtale_bot::IncomingMessage>(256);
    let (bot_response_tx, mut bot_response_rx) =
        tokio::sync::mpsc::channel::<springtale_bot::OutgoingResponse>(256);
    // No firing-rule ingress in the desktop chat path; keep the rx alive.
    let (_bot_rule_tx, bot_rule_rx) =
        tokio::sync::mpsc::channel::<springtale_core::rule::engine::TriggerEvent>(256);
    // No live formations roster yet on the desktop — an empty handle is fine;
    // chat doesn't depend on it (formation orchestration lands with the
    // canvas wiring).
    let formations_handle = Arc::new(RwLock::new(Vec::new()));

    // Conversational task-setup deploy port (apply + schedule a recipe
    // the user configured by chatting — no AI needed).
    let recipe_deployer = Arc::new(
        springtale_bot::conversation::deploy::RuntimeRecipeDeployer::new(
            runtime.clone(),
            scheduler,
        ),
    );

    let bot = springtale_bot::BotBuilder::new()
        .recipe_deployer(recipe_deployer)
        .store(runtime.store.clone())
        .registry(runtime.registry.clone())
        .engine(runtime.engine.clone())
        .ai_adapter((**runtime.ai_adapter.load()).clone())
        .sentinel(runtime.sentinel.clone())
        .config(springtale_bot::BotConfig::default())
        .connector_rx(bot_msg_rx)
        .rule_rx(bot_rule_rx)
        .response_tx(bot_response_tx)
        .formation_cmd_rx(formation_cmd_rx)
        .formations_handle(formations_handle)
        .role_registry(runtime.role_registry.clone())
        .capability_bridge(runtime.capability_bridge.clone())
        .canvas_tx(runtime.canvas_tx.clone())
        .cooperation_tx(runtime.cooperation_tx.clone())
        .formation_gossip(runtime.formation_gossip.clone())
        .knowledge_store(runtime.knowledge_store.clone())
        .build()
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("[in-app-chat] embedded bot event loop started");
    tauri::async_runtime::spawn(async move {
        bot.start().await;
        tracing::warn!("[in-app-chat] bot event loop EXITED");
    });

    // Delivery forwarder: a fired Notify/SendMessage step is broadcast
    // on `runtime.notification_tx` by the embedded job consumer. Mirror
    // it to the chat panel (`chat-message` event) AND a best-effort OS
    // notification so a scheduled recipe (weather briefing, hydration
    // reminder, …) reaches the user even when the window isn't focused.
    let notif_app = app.clone();
    let mut notif_rx = runtime.notification_tx.subscribe();
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_notification::NotificationExt;
        loop {
            match notif_rx.recv().await {
                Ok(event) => {
                    let body = if event.body.is_empty() {
                        event.title.clone()
                    } else {
                        format!("{}\n{}", event.title, event.body)
                    };
                    // Chat panel render.
                    if let Err(e) = notif_app.emit(
                        "chat-message",
                        ChatEvent {
                            session: "in-app".to_owned(),
                            text: body.clone(),
                        },
                    ) {
                        tracing::error!(error = %e, "[notification] chat emit failed");
                    }
                    // Best-effort native notification.
                    if let Err(e) = notif_app
                        .notification()
                        .builder()
                        .title(&event.title)
                        .body(&event.body)
                        .show()
                    {
                        tracing::debug!(error = %e, "[notification] OS notification failed");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "[notification] forwarder lagged");
                }
            }
        }
    });

    // Response dispatcher: in-app replies → `chat-message` events. Any other
    // connector target is dropped (the desktop has no gateways to send through).
    tauri::async_runtime::spawn(async move {
        while let Some(response) = bot_response_rx.recv().await {
            tracing::info!(
                connector = %response.connector,
                text = %response.text,
                "[in-app-chat] ← bot reply"
            );
            if response.connector == "in-app" {
                match app.emit(
                    "chat-message",
                    ChatEvent {
                        session: response.channel_id,
                        text: response.text,
                    },
                ) {
                    Ok(()) => tracing::info!("[in-app-chat] emitted chat-message event"),
                    Err(e) => tracing::error!(error = %e, "[in-app-chat] emit failed"),
                }
            }
        }
    });

    Ok(bot_msg_tx)
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
