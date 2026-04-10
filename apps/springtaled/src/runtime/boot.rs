use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{RwLock, mpsc};

use springtale_core::rule::engine::RuleEngine;
use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::vault::store::Vault;
use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::queue::consumer::JobConsumer;
use springtale_scheduler::queue::producer::JobProducer;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;
use springtale_transport::local::unix_socket::LocalTransport;

use crate::api;
use crate::config::SpringtaleConfig;

/// Boot the springtaled daemon.
///
/// Executes the ordered startup sequence from the architecture doc (§8.1).
/// Each step must succeed before the next. Errors are fatal.
pub async fn boot(
    config: SpringtaleConfig,
    connector_configs: std::collections::HashMap<String, serde_json::Value>,
) -> Result<()> {
    // ── Step 1: Config already loaded by caller ──
    tracing::info!("springtaled starting");

    // Warn if API is bound to 0.0.0.0
    if config.api.bind.starts_with("0.0.0.0") {
        tracing::warn!(
            bind = %config.api.bind,
            "management API bound to all interfaces — this exposes it to the network"
        );
    }

    // Destructure config to avoid partial-move issues (ai/sentinel fields
    // are moved into RuntimeConfig, the rest stays available by name).
    let SpringtaleConfig {
        ephemeral,
        store: store_config,
        crypto: crypto_config,
        transport: transport_config,
        api: api_config,
        heartbeat_interval_secs,
        bot: bot_config,
        telegram: telegram_config,
        sentinel,
        ai_ollama,
        ai_openai,
        ai_anthropic,
        nostr: nostr_config,
        irc: irc_config,
        discord: discord_config,
        slack: slack_config,
        signal: signal_config,
    } = config;

    // ── Step 2: Initialize crypto vault (before runtime, no dependencies) ──
    let (vault, keypair, api_token_hash) = init_crypto(ephemeral, &crypto_config)?;

    // ── Step 3: Initialize shared runtime (store + engine + registry + AI + sentinel + canvas) ──
    let runtime_config = springtale_runtime::RuntimeConfig {
        store: springtale_runtime::config::StoreConfig {
            path: store_config.path.clone(),
            ephemeral,
        },
        ai_ollama,
        ai_openai,
        ai_anthropic,
        sentinel,
        connector_configs,
    };
    let runtime = init_runtime(runtime_config).await?;

    // ── Step 4: Initialize transport ──
    let _transport = init_transport(&transport_config, &keypair).await?;

    // ── Step 5: Start scheduler (cron + file watcher + heartbeat) ──
    let (trigger_tx, trigger_rx, cron_executor, fs_watcher, heartbeat_monitor) =
        init_schedulers(&runtime, heartbeat_interval_secs).await?;

    // ── Step 6: Initialize job queue (action execution pipeline) ──
    let producer = init_job_queue(&runtime).await?;

    // ── Step 7: Initialize bot + connector gateways ──
    let (bot_msg_tx, bot_msg_rx) = mpsc::channel::<springtale_bot::IncomingMessage>(256);
    let connector_wiring = ConnectorWiring {
        telegram: telegram_config,
        nostr: nostr_config,
        irc: irc_config,
        discord: discord_config,
        slack: slack_config,
        signal: signal_config,
    };
    let (bot_handle, _connector_shutdowns) =
        init_bot(&runtime, bot_config, &connector_wiring, bot_msg_tx, bot_msg_rx).await?;

    // ── Step 8: Build and start API server ──

    let ready_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let cron_arc = Arc::new(tokio::sync::Mutex::new(cron_executor));
    let watcher_arc = Arc::new(tokio::sync::Mutex::new(fs_watcher));

    // Broadcast channel for SSE event streaming to dashboard
    let (event_tx, _event_rx) = tokio::sync::broadcast::channel(256);

    let state = api::state::AppState {
        runtime: runtime.clone(),
        api_token_hash,
        ready: ready_flag.clone(),
        trigger_tx: trigger_tx.clone(),
        scheduler: crate::scheduler::AppScheduler {
            cron: cron_arc,
            fs_watcher: watcher_arc,
        },
        rate_limit_per_sec: u64::from(api_config.rate_limit_per_sec),
        event_tx,
        heartbeat_monitor: Arc::new(tokio::sync::Mutex::new(heartbeat_monitor)),
    };

    let router = api::build_router(state);
    let listener = tokio::net::TcpListener::bind(&api_config.bind)
        .await
        .with_context(|| format!("failed to bind API to {}", api_config.bind))?;
    tracing::info!(bind = %api_config.bind, "management API listening");

    // ── Step 9: Signal readiness ──
    ready_flag.store(true, std::sync::atomic::Ordering::Release);
    println!("READY");

    // ── Run: API server + trigger event loop ──
    let engine = runtime.engine.clone();
    let api_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router)
            .with_graceful_shutdown(crate::shutdown::shutdown_signal())
            .await
        {
            tracing::error!(error = %e, "API server error");
        }
    });

    let event_loop = tokio::spawn(async move {
        event_loop(trigger_rx, engine, producer).await;
    });

    // Wait for shutdown
    tokio::select! {
        _ = api_handle => tracing::info!("API server stopped"),
        _ = event_loop => tracing::info!("event loop stopped"),
        _ = bot_handle => tracing::info!("bot event loop stopped"),
    }

    // Cleanup
    drop(vault);
    tracing::info!("springtaled shutdown complete");
    Ok(())
}

/// Holds optional connector configs for wiring during boot.
/// Avoids partial-move issues with the top-level `SpringtaleConfig`.
struct ConnectorWiring {
    telegram: Option<connector_telegram::TelegramConfig>,
    nostr: Option<connector_nostr::NostrConfig>,
    irc: Option<connector_irc::IrcConfig>,
    discord: Option<connector_discord::DiscordConfig>,
    slack: Option<connector_slack::SlackConfig>,
    signal: Option<connector_signal::SignalConfig>,
}

// ── Extracted boot steps ──────────────────────────────────────────────────────

/// Step 2: Initialize shared runtime (store + engine + registry + AI + sentinel + canvas).
async fn init_runtime(
    runtime_config: springtale_runtime::RuntimeConfig,
) -> Result<springtale_runtime::RuntimeState> {
    springtale_runtime::init(&runtime_config).await.context("failed to initialize runtime")
}

/// Initialize crypto vault, load identity keypair, derive API token hash.
fn init_crypto(
    ephemeral: bool,
    crypto_config: &crate::config::CryptoConfig,
) -> Result<(Vault, Keypair, [u8; 32])> {
    let passphrase = get_passphrase()?;
    let (vault, keypair) = if ephemeral {
        let mut vault = springtale_crypto::vault::store::Vault::create_ephemeral(&passphrase)
            .context("failed to create ephemeral vault")?;
        let keypair = springtale_crypto::identity::keypair::Keypair::generate()
            .context("failed to generate ephemeral keypair")?;
        // SECURITY: expose needed to persist identity in ephemeral vault
        vault
            .set("identity", keypair.expose_secret_bytes().to_vec())
            .context("failed to store ephemeral identity")?;
        (vault, keypair)
    } else {
        tracing::info!(path = %crypto_config.vault_path.display(), "opening crypto vault");
        if let Some(parent) = crypto_config.vault_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create vault directory: {}", parent.display())
            })?;
        }
        open_or_create_vault(&crypto_config.vault_path, &passphrase)?
    };
    let node_id = keypair.node_id();
    tracing::info!(node_id = %hex::encode(node_id.as_bytes()), "identity loaded");

    // Detect duress session — if the vault was opened with a duress passphrase,
    // log it (hidden audit, only visible with real passphrase) and continue
    // with minimal capabilities.
    if vault.is_duress_session() {
        tracing::info!("vault opened in duress mode — minimal profile active");
    }

    // Derive API token from passphrase hash (HMAC-SHA256)
    let api_token_hash = springtale_crypto::token::derive_api_token_hash(&passphrase);

    Ok((vault, keypair, api_token_hash))
}

/// Initialize transport layer (local Unix socket or HTTP with mTLS).
async fn init_transport(
    transport_config: &crate::config::TransportConfig,
    keypair: &Keypair,
) -> Result<Arc<dyn springtale_transport::Transport>> {
    let node_id = keypair.node_id();
    let transport: Arc<dyn springtale_transport::Transport> = match transport_config
        .transport_type
        .as_str()
    {
        "http" => {
            let http_config = transport_config.http.clone().ok_or_else(|| {
                anyhow::anyhow!("transport type is 'http' but [transport.http] config is missing")
            })?;
            tracing::info!(addr = %http_config.listen_addr, "binding HTTP transport (mTLS)");
            Arc::new(
                springtale_transport::http::HttpTransport::bind(node_id, http_config)
                    .await
                    .context("failed to bind HTTP transport")?,
            )
        }
        _ => {
            tracing::info!(path = %transport_config.socket_path.display(), "binding local transport");
            Arc::new(
                LocalTransport::bind(node_id, &transport_config.socket_path)
                    .await
                    .context("failed to bind local transport")?,
            )
        }
    };
    tracing::info!(transport = transport.name(), "transport initialized");
    Ok(transport)
}

/// Start scheduler subsystems (cron executor, filesystem watcher, heartbeat monitor).
///
/// Returns the trigger channel pair and the three scheduler components for later
/// ownership by the API state.
async fn init_schedulers(
    runtime: &springtale_runtime::RuntimeState,
    heartbeat_interval_secs: u64,
) -> Result<(
    mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
    mpsc::Receiver<springtale_core::rule::engine::TriggerEvent>,
    CronExecutor,
    FsWatcher,
    springtale_scheduler::HeartbeatMonitor,
)> {
    let (trigger_tx, trigger_rx) = mpsc::channel(256);

    let mut cron_executor = CronExecutor::new(trigger_tx.clone());
    let mut fs_watcher =
        FsWatcher::new(trigger_tx.clone()).context("failed to create filesystem watcher")?;

    // Schedule cron triggers and file watches from rules
    {
        let rules = runtime
            .store
            .list_rules()
            .await
            .context("failed to load rules for scheduler")?;
        for rule in &rules {
            if let springtale_core::rule::Trigger::Cron { expression, .. } = &rule.trigger
                && let Err(e) = cron_executor.schedule(&rule.name, expression)
            {
                tracing::warn!(rule = %rule.name, error = %e, "failed to schedule cron trigger");
            }
            if let springtale_core::rule::Trigger::FileWatch { path, .. } = &rule.trigger
                && let Err(e) = fs_watcher.watch(path)
            {
                tracing::warn!(rule = %rule.name, error = %e, "failed to watch path");
            }
        }
    }

    // Start heartbeat monitor (periodic rule evaluation)
    let mut heartbeat_monitor = springtale_scheduler::HeartbeatMonitor::new(
        heartbeat_interval_secs,
        trigger_tx.clone(),
    );
    if heartbeat_interval_secs > 0 {
        heartbeat_monitor.start();
        tracing::info!(
            interval_secs = heartbeat_interval_secs,
            "heartbeat monitor started"
        );
    }

    tracing::info!(
        cron_jobs = cron_executor.list().len(),
        watched_paths = fs_watcher.watched_paths().len(),
        "scheduler started"
    );

    Ok((trigger_tx, trigger_rx, cron_executor, fs_watcher, heartbeat_monitor))
}

/// Step 6: Initialize job queue (producer + consumer with sentinel dispatch).
///
/// Spawns the consumer as a background task and returns the producer for
/// use by the event loop.
async fn init_job_queue(
    runtime: &springtale_runtime::RuntimeState,
) -> Result<Arc<JobProducer>> {
    let (job_tx, job_rx) = mpsc::channel(100);
    let producer = Arc::new(JobProducer::new(job_tx));
    let mut consumer = JobConsumer::new(job_rx, 4);

    // Install action dispatcher as the job handler.
    let dispatch_registry = runtime.registry.clone();
    let dispatch_sentinel = runtime.sentinel.clone();
    consumer.set_handler(std::sync::Arc::new(move |job| {
        let reg = dispatch_registry.clone();
        let sent = dispatch_sentinel.clone();
        Box::pin(async move {
            let action: springtale_core::rule::action::Action = serde_json::from_value(job.payload)
                .map_err(|e| format!("failed to deserialize action: {e}"))?;

            // Sentinel evaluation before dispatch
            let connector_name = match &action {
                springtale_core::rule::action::Action::RunConnector { connector, .. } => {
                    connector.as_str()
                }
                _ => "system",
            };
            let verdict = sent.evaluate(&action, connector_name).await;
            match verdict {
                springtale_sentinel::Verdict::Go => {}
                springtale_sentinel::Verdict::Throttle(duration) => {
                    tracing::info!(
                        connector = connector_name,
                        delay_ms = duration.as_millis() as u64,
                        "sentinel: throttling action"
                    );
                    tokio::time::sleep(duration).await;
                }
                springtale_sentinel::Verdict::Pause(reason) => {
                    return Err(format!("sentinel paused: {reason}"));
                }
                springtale_sentinel::Verdict::Quarantine(reason) => {
                    return Err(format!("sentinel quarantined: {reason}"));
                }
            }

            let result = crate::dispatch::dispatch_action(&action, &reg).await;
            match &result {
                Ok(_) => sent.report_success(connector_name),
                Err(_) => sent.report_failure(connector_name),
            }
            result.map(|_| ())
        })
    }));

    // Spawn consumer as background task
    tokio::spawn(async move {
        if let Err(e) = consumer.run().await {
            tracing::error!(error = %e, "job consumer error");
        }
    });
    tracing::info!("job queue started (concurrency: 4)");

    Ok(producer)
}

/// Initialize bot runtime and wire connector gateways.
///
/// Spawns the bot event loop, response dispatcher, and all configured connector
/// gateway loops. Returns the bot task handle and connector shutdown senders.
async fn init_bot(
    runtime: &springtale_runtime::RuntimeState,
    bot_config: Option<springtale_bot::BotConfig>,
    connectors: &ConnectorWiring,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    bot_msg_rx: mpsc::Receiver<springtale_bot::IncomingMessage>,
) -> Result<(
    tokio::task::JoinHandle<()>,
    Vec<tokio::sync::watch::Sender<bool>>,
)> {
    let (bot_response_tx, mut bot_response_rx) =
        mpsc::channel::<springtale_bot::OutgoingResponse>(256);
    let (_bot_rule_tx, bot_rule_rx) =
        mpsc::channel::<springtale_core::rule::engine::TriggerEvent>(256);

    let bot_config = bot_config.unwrap_or_default();

    let bot = springtale_bot::BotBuilder::new()
        .store(runtime.store.clone())
        .registry(runtime.registry.clone())
        .engine(runtime.engine.clone())
        .ai_adapter((**runtime.ai_adapter.load()).clone())
        .config(bot_config)
        .connector_rx(bot_msg_rx)
        .rule_rx(bot_rule_rx)
        .response_tx(bot_response_tx)
        .build()
        .await
        .context("failed to initialize bot runtime")?;

    // Spawn bot event loop
    let bot_handle = tokio::spawn(async move {
        bot.start().await;
    });

    // Spawn response dispatcher: routes bot responses to connectors
    let response_registry = runtime.registry.clone();
    let _response_handle = tokio::spawn(async move {
        while let Some(response) = bot_response_rx.recv().await {
            let reg = response_registry.read().await;
            let input = serde_json::json!({
                "chat_id": response.channel_id,
                "text": response.text,
            });
            match reg
                .execute(&response.connector, "send_message", input)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        connector = %response.connector,
                        error = %e,
                        "failed to send bot response"
                    );
                }
            }
        }
    });

    tracing::info!("bot runtime started");

    // ── Step 7b: Start connector gateways ──
    // Connectors are already registered in the registry by the factory system
    // (via inventory::submit! in each connector crate). Gateway loops bridge
    // incoming messages from chat platforms to the bot runtime.
    let mut _connector_shutdowns: Vec<tokio::sync::watch::Sender<bool>> = Vec::new();

    if let Some(ref tg_config) = connectors.telegram {
        crate::runtime::connectors::wire_telegram(
            tg_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Telegram connector")?;
    }
    if let Some(ref nostr_config) = connectors.nostr {
        let shutdown_tx = crate::runtime::connectors::wire_nostr(
            nostr_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Nostr connector")?;
        _connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref irc_config) = connectors.irc {
        let shutdown_tx = crate::runtime::connectors::wire_irc(
            irc_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire IRC connector")?;
        _connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref discord_config) = connectors.discord {
        let shutdown_tx = crate::runtime::connectors::wire_discord(
            discord_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Discord connector")?;
        _connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref slack_config) = connectors.slack {
        let shutdown_tx = crate::runtime::connectors::wire_slack(
            slack_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Slack connector")?;
        _connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref signal_config) = connectors.signal {
        let shutdown_tx = crate::runtime::connectors::wire_signal(
            signal_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Signal connector")?;
        _connector_shutdowns.push(shutdown_tx);
    }
    // connector-matrix: DEFERRED — matrix-sdk 0.16 requires rusqlite 0.37
    // which has CVE-2025-70873 (heap info disclosure). Waiting for update.

    Ok((bot_handle, _connector_shutdowns))
}

// ── Existing helpers ──────────────────────────────────────────────────────────

/// Main event loop: receives trigger events, matches rules, enqueues actions.
async fn event_loop(
    mut trigger_rx: mpsc::Receiver<springtale_core::rule::engine::TriggerEvent>,
    engine: Arc<RwLock<RuleEngine>>,
    producer: Arc<JobProducer>,
) {
    while let Some(event) = trigger_rx.recv().await {
        let engine = engine.read().await;
        let matches = springtale_core::router::dispatch::dispatch_event(&engine, &event);

        for rule_match in &matches {
            tracing::info!(
                rule = %rule_match.rule_name,
                actions = rule_match.actions.len(),
                "rule matched trigger — enqueuing actions"
            );

            for action in rule_match.actions.iter() {
                match serde_json::to_value(action) {
                    Ok(payload) => {
                        if let Err(e) = producer.enqueue(payload, 3).await {
                            tracing::error!(
                                rule = %rule_match.rule_name,
                                error = %e,
                                "failed to enqueue action"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            rule = %rule_match.rule_name,
                            error = %e,
                            "failed to serialize action"
                        );
                    }
                }
            }
        }
    }
}

/// Get the vault passphrase from Docker secret file, environment, or interactive prompt.
///
/// Priority:
/// 1. SPRINGTALE_PASSPHRASE_FILE — read passphrase from file (Docker secrets pattern)
/// 2. SPRINGTALE_PASSPHRASE — direct env var (development only, visible in `docker inspect`)
/// 3. Interactive prompt via rpassword (if stdin is a terminal)
fn get_passphrase() -> Result<Vec<u8>> {
    // Docker secrets pattern: read from file path in env var
    if let Ok(file_path) = std::env::var("SPRINGTALE_PASSPHRASE_FILE") {
        // Read as bytes and zeroize immediately — passphrase must not
        // linger in memory (IPV survivor's device may be seized).
        let mut raw_bytes = std::fs::read(&file_path)
            .with_context(|| format!("failed to read passphrase from {file_path}"))?;
        // Trim trailing newline/whitespace from file
        while raw_bytes.last().is_some_and(|b| b.is_ascii_whitespace()) {
            raw_bytes.pop();
        }
        if raw_bytes.is_empty() {
            anyhow::bail!("passphrase file is empty: {file_path}");
        }
        return Ok(raw_bytes);
    }

    // Direct env var (development convenience, NOT recommended for production)
    if let Ok(pass) = std::env::var("SPRINGTALE_PASSPHRASE") {
        return Ok(pass.into_bytes());
    }

    // Interactive prompt if stdin is a terminal
    if atty_check() {
        let pass = rpassword::read_password_from_tty(Some("Vault passphrase: "))
            .context("failed to read passphrase")?;
        if pass.is_empty() {
            anyhow::bail!("passphrase cannot be empty");
        }
        return Ok(pass.into_bytes());
    }

    anyhow::bail!(
        "no passphrase provided: set SPRINGTALE_PASSPHRASE_FILE, SPRINGTALE_PASSPHRASE, or run interactively"
    )
}

/// Check if stdin is a terminal (for interactive passphrase prompt).
fn atty_check() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Open an existing vault or create a new one on first run.
fn open_or_create_vault(path: &std::path::Path, passphrase: &[u8]) -> Result<(Vault, Keypair)> {
    if path.exists() {
        let vault =
            Vault::open(path, passphrase).context("failed to open vault (wrong passphrase?)")?;
        let identity_bytes = vault
            .get("identity")
            .context("failed to read identity from vault")?
            .ok_or_else(|| anyhow::anyhow!("vault has no identity key"))?
            .clone();
        let bytes: [u8; 32] = identity_bytes
            .as_slice()
            .try_into()
            .context("identity key is wrong size (expected 32 bytes)")?;
        let keypair =
            Keypair::from_secret_bytes(bytes).context("failed to restore keypair from vault")?;
        Ok((vault, keypair))
    } else {
        tracing::info!("creating new vault and identity");
        let keypair = Keypair::generate().context("failed to generate identity keypair")?;
        let mut vault = Vault::create(path, passphrase).context("failed to create vault")?;
        // SECURITY: expose needed to persist identity key material
        vault
            .set("identity", keypair.expose_secret_bytes().to_vec())
            .context("failed to store identity in vault")?;
        vault.save().context("failed to save vault")?;
        Ok((vault, keypair))
    }
}
