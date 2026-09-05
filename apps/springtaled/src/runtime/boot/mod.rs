mod bot;
mod crypto;
mod formations;
mod sentinel;
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
        bluesky: bluesky_config,
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
        // Default cooperation config is single-process in-memory gossip.
        // Cross-process (chitchat) is opt-in via springtaled.toml:
        //     [cooperation]
        //     cross_process = true
        //     chitchat_listen_addr = "127.0.0.1:18000"
        //     chitchat_seeds = ["127.0.0.1:18001"]
        cooperation: springtale_runtime::config::CooperationConfig::default(),
    };
    // Formation command channel: sender goes to runtime (operations send commands),
    // receiver goes to bot (event loop materializes/removes formations).
    let (formation_cmd_tx, formation_cmd_rx) =
        tokio::sync::mpsc::channel::<springtale_cooperation::command::FormationCommand>(32);
    // Kept for restoring persisted formations after `init_bot` spawns the
    // event loop that owns `formation_cmd_rx` below (§6.11 / finding 119).
    let formation_cmd_tx_for_restore = formation_cmd_tx.clone();

    // Create the shared formations handle BEFORE runtime init.
    // The BotBuilder will use this same Arc, and BotFormationReader reads from it.
    let formations_handle = Arc::new(tokio::sync::RwLock::new(Vec::new()));
    let live_reader: Option<Arc<dyn springtale_runtime::LiveFormationReader>> = Some(Arc::new(
        bot::BotFormationReader::new(formations_handle.clone()),
    ));

    // springtaled is the headless daemon — no UI gate to prompt the
    // user, so leave `approval_gate: None`. The sentinel falls back
    // to `DefaultDenyApprovalGate` per W1.F design. The desktop wraps
    // springtaled via Tauri and supplies its own `ChannelApprovalGate`.
    let runtime = springtale_runtime::init(&runtime_config, formation_cmd_tx, live_reader, None)
        .await
        .context("failed to initialize runtime")?;

    // ── Step 3b: Verify audit-log row-hash chain ──
    // Tamper-evident audit trail (Phase-7 Finding B): walk every row
    // in `audit_trail`, recompute the SHA-256 chain, and fail closed
    // on any mismatch. The chain's genesis is bound to the vault
    // identity — a different vault on the same SQLite or a tampered
    // row both refuse to start.
    sentinel::verify_audit_chain(&runtime.store, &keypair)
        .await
        .context("audit chain verification failed at startup")?;

    // ── Step 4: Initialize transport ──
    let _transport = transport::init_transport(&transport_config, &keypair).await?;

    // ── Step 5/6: Start scheduler + job queue + trigger event loop ──
    // Shared bootstrap with the desktop app (CLAUDE.md: "The desktop
    // app IS springtaled with a GUI. Same runtime underneath."). Both
    // surfaces now drive identical cron/fs_watcher/queue/event-loop
    // wiring from `springtale_runtime::embedded::bootstrap`.
    let springtale_runtime::EmbeddedBootHandle {
        scheduler: embedded_scheduler,
        heartbeat_monitor,
    } = springtale_runtime::bootstrap_embedded(&runtime, heartbeat_interval_secs)
        .await
        .map_err(|e| anyhow::anyhow!("scheduler bootstrap failed: {e}"))?;
    let trigger_tx = embedded_scheduler.trigger_tx.clone();

    // ── Step 7: Initialize bot + connector gateways ──
    let (bot_msg_tx, bot_msg_rx) = mpsc::channel::<springtale_bot::IncomingMessage>(256);
    // Clone for AppState so webhook handlers can route chat messages to the bot
    // in webhook mode (polling gateways use the original sender directly).
    let api_bot_msg_tx = bot_msg_tx.clone();
    // W5 in-app chat broadcast — created here so both the bot response
    // dispatcher (producer) and AppState's `GET /chat/stream` (consumers)
    // share the same channel.
    let (chat_tx, _chat_rx) = tokio::sync::broadcast::channel::<api::chat::ChatStreamMessage>(256);
    let connector_wiring = bot::ConnectorWiring {
        bluesky: bluesky_config,
        telegram: telegram_config,
        nostr: nostr_config,
        irc: irc_config,
        discord: discord_config,
        slack: slack_config,
        signal: signal_config,
    };
    let (bot_handle, connector_shutdowns) = bot::init_bot(
        &runtime,
        embedded_scheduler.clone(),
        bot_config,
        &connector_wiring,
        bot::BotChannels {
            bot_msg_tx,
            bot_msg_rx,
            chat_tx: chat_tx.clone(),
        },
        formation_cmd_rx,
        formations_handle,
    )
    .await?;

    // ── Step 7a2: Restore formations persisted from a previous run ──
    // `init_bot` has already spawned the event loop that owns
    // `formation_cmd_rx`, so these sends queue behind it rather than
    // blocking boot (§6.11 / finding 119).
    let formations_restored =
        formations::restore_formations(&runtime.store, &formation_cmd_tx_for_restore).await?;
    tracing::info!(
        formations_restored,
        "formation restore step complete at boot"
    );

    // ── Step 7b: ConnectorEvent handlers are wired inside
    // `bootstrap_embedded` (shared with desktop), which publishes the
    // registry on `RuntimeState`. Clone it for AppState so the rule CRUD
    // handlers attach/detach through the same instance.
    let trigger_registry = runtime.trigger_registry.get().cloned().unwrap_or_else(|| {
        springtale_runtime::TriggerRegistry::new(trigger_tx.clone(), runtime.store.clone())
    });

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

    // Broadcast channel for SSE event streaming to dashboard
    let (event_tx, _event_rx) = tokio::sync::broadcast::channel(256);

    let state = api::state::AppState {
        runtime: runtime.clone(),
        api_token_hash,
        ready: ready_flag.clone(),
        trigger_tx: trigger_tx.clone(),
        scheduler: embedded_scheduler,
        rate_limit_per_sec: u64::from(api_config.rate_limit_per_sec),
        event_tx,
        heartbeat_monitor,
        trigger_registry,
        bot_msg_tx: api_bot_msg_tx,
        chat_tx,
    };

    let router = api::build_router(state);
    let listener = tokio::net::TcpListener::bind(&api_config.bind)
        .await
        .with_context(|| format!("failed to bind API to {}", api_config.bind))?;
    tracing::info!(bind = %api_config.bind, "management API listening");

    // ── Step 9: Signal readiness ──
    ready_flag.store(true, std::sync::atomic::Ordering::Release);
    println!("READY");

    // ── Run: API server (cron + queue + event loop run inside the
    //         shared `bootstrap_embedded` from springtale-runtime) ──
    let api_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router)
            .with_graceful_shutdown(crate::shutdown::shutdown_signal())
            .await
        {
            tracing::error!(error = %e, "API server error");
        }
    });

    // Wait for shutdown
    tokio::select! {
        _ = api_handle => tracing::info!("API server stopped"),
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
