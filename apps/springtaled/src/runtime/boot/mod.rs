mod bot;
pub mod connector_events;
mod crypto;
mod event_loop;
mod queue;
mod schedulers;
mod transport;

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::mpsc;

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
    let (vault, keypair, api_token_hash, db_key_hex) =
        crypto::init_crypto(ephemeral, &crypto_config)?;

    // ── Step 3: Initialize shared runtime (store + engine + registry + AI + sentinel + canvas) ──
    let runtime_config = springtale_runtime::RuntimeConfig {
        store: springtale_runtime::config::StoreConfig {
            path: store_config.path.clone(),
            ephemeral,
            encryption_key_hex: if ephemeral { None } else { Some(db_key_hex) },
            retention_days: store_config.retention_days,
        },
        ai_ollama,
        ai_openai,
        ai_anthropic,
        sentinel,
        connector_configs,
    };
    let runtime = springtale_runtime::init(&runtime_config)
        .await
        .context("failed to initialize runtime")?;

    // ── Step 4: Initialize transport ──
    let _transport = transport::init_transport(&transport_config, &keypair).await?;

    // ── Step 5: Start scheduler (cron + file watcher + heartbeat) ──
    let (trigger_tx, trigger_rx, cron_executor, fs_watcher, heartbeat_monitor) =
        schedulers::init_schedulers(&runtime, heartbeat_interval_secs).await?;

    // ── Step 6: Initialize job queue (action execution pipeline) ──
    let producer = queue::init_job_queue(&runtime).await?;

    // ── Step 7: Initialize bot + connector gateways ──
    let (bot_msg_tx, bot_msg_rx) = mpsc::channel::<springtale_bot::IncomingMessage>(256);
    // Clone for AppState so webhook handlers can route chat messages to the bot
    // in webhook mode (polling gateways use the original sender directly).
    let api_bot_msg_tx = bot_msg_tx.clone();
    let connector_wiring = bot::ConnectorWiring {
        telegram: telegram_config,
        nostr: nostr_config,
        irc: irc_config,
        discord: discord_config,
        slack: slack_config,
        signal: signal_config,
    };
    let (bot_handle, connector_shutdowns) = bot::init_bot(
        &runtime,
        bot_config,
        &connector_wiring,
        bot_msg_tx,
        bot_msg_rx,
    )
    .await?;

    // ── Step 7b: Wire connector event handlers for ConnectorEvent rules ──
    let trigger_registry = connector_events::wire_connector_events(
        &runtime.registry,
        &runtime.engine,
        trigger_tx.clone(),
    )
    .await;

    // ── Step 7c: Start data retention purge (if configured) ──
    if let Some(days) = store_config.retention_days {
        let purge_store = runtime.store.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                if let Err(e) =
                    springtale_runtime::operations::data::purge_expired_data(&*purge_store, days)
                        .await
                {
                    tracing::warn!(error = %e, "data retention purge failed");
                }
            }
        });
        tracing::info!(
            retention_days = days,
            "data retention purge started (hourly)"
        );
    }

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
        trigger_registry,
        bot_msg_tx: api_bot_msg_tx,
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

    let event_loop_handle = tokio::spawn(async move {
        event_loop::event_loop(trigger_rx, engine, producer).await;
    });

    // Wait for shutdown
    tokio::select! {
        _ = api_handle => tracing::info!("API server stopped"),
        _ = event_loop_handle => tracing::info!("event loop stopped"),
        _ = bot_handle => tracing::info!("bot event loop stopped"),
    }

    // Signal every connector gateway (Telegram polling, Discord, IRC, ...)
    // to drain its in-flight work and exit. Without this, tasks that own
    // persistent WebSocket/polling loops keep running until the runtime
    // is dropped, which can leave outbound messages half-sent. The plan
    // called this out explicitly: "Telegram shutdown handle lost (no
    // graceful stop)".
    for tx in &connector_shutdowns {
        if let Err(e) = tx.send(true) {
            tracing::warn!(error = %e, "connector shutdown channel already closed");
        }
    }

    // Cleanup
    drop(vault);
    tracing::info!("springtaled shutdown complete");
    Ok(())
}
