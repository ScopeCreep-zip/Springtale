//! The boot pipeline, as one reusable function.
//!
//! Cold boot and `POST /vault/unlock` build the same thing: an open
//! vault, an initialized runtime, a scheduler, a bot, and a router over
//! them. Before plan 6.10 that sequence only existed inline in
//! [`super::boot`], which is why unlocking a locked daemon was not
//! possible — there was nothing to call. [`build_live`] is that
//! sequence, and both callers go through it.
//!
//! The only difference between the two callers is where the passphrase
//! came from: a TTY / environment / stdin at boot, a request body at
//! unlock.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api;
use crate::api::lock::Live;
use crate::config::{ApiConfig, CryptoConfig, StoreConfig, TransportConfig};

use super::{bot, crypto, formations, sentinel, transport};

/// Everything [`build_live`] needs that does not change between a boot
/// and an unlock.
///
/// Held behind an `Arc` by the unlock closure, so the daemon keeps the
/// configuration it was started with for the life of the process — a
/// lock does not re-read `springtale.toml`, and cannot be made to pick
/// up a file an attacker edited while the vault was closed.
pub struct BootContext {
    /// Ephemeral mode: everything in memory, nothing on disk.
    pub ephemeral: bool,
    pub store: StoreConfig,
    pub crypto: CryptoConfig,
    pub transport: TransportConfig,
    pub api: ApiConfig,
    pub heartbeat_interval_secs: u64,
    pub sentinel: Option<springtale_sentinel::SentinelConfig>,
    /// Per-connector `[telegram]` / `[discord]` / … tables, verbatim.
    pub connector_configs: HashMap<String, serde_json::Value>,
}

/// Open the vault and build a complete, serving runtime from it.
///
/// Every `Arc` this creates is reachable from the returned [`Live`], so
/// dropping it is what closes the database and zeroizes the key. That
/// is the whole reason locking works.
pub async fn build_live(ctx: &BootContext, passphrase: &[u8]) -> Result<Live> {
    // ── Initialize crypto vault (before runtime, no dependencies) ──
    let crypto::CryptoBoot {
        vault,
        keypair,
        api_token_hash,
        db_key_hex,
    } = crypto::init_crypto(ctx.ephemeral, &ctx.crypto, passphrase)?;

    // ── Initialize shared runtime (store + engine + registry + AI + sentinel + canvas) ──
    let runtime_config = springtale_runtime::RuntimeConfig {
        store: springtale_runtime::config::StoreConfig {
            path: ctx.store.path.clone(),
            ephemeral: ctx.ephemeral,
            encryption_key_hex: if ctx.ephemeral {
                None
            } else {
                Some(db_key_hex)
            },
            retention_days: ctx.store.retention_days,
        },
        sentinel: ctx.sentinel.clone(),
        connector_configs: ctx.connector_configs.clone(),
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

    // ── Verify audit-log row-hash chain ──
    // Tamper-evident audit trail (Phase-7 Finding B): walk every row
    // in `audit_trail`, recompute the SHA-256 chain, and fail closed
    // on any mismatch. The chain's genesis is bound to the vault
    // identity — a different vault on the same SQLite or a tampered
    // row both refuse to start.
    sentinel::verify_audit_chain(&runtime.store, &keypair)
        .await
        .context("audit chain verification failed at startup")?;

    // ── Initialize transport ──
    let transport = transport::init_transport(&ctx.transport, &keypair).await?;

    // ── Start scheduler + job queue + trigger event loop ──
    // Shared bootstrap with the desktop app (CLAUDE.md: "The desktop
    // app IS springtaled with a GUI. Same runtime underneath."). Both
    // surfaces now drive identical cron/fs_watcher/queue/event-loop
    // wiring from `springtale_runtime::embedded::bootstrap`. Those
    // tasks register on `runtime.tasks`, so a lock ends them.
    // The heartbeat interval is durable config: a `PUT /config/heartbeat`
    // persists it, so boot reads the stored key back and only falls
    // through to the config-file value when nothing was ever set.
    let heartbeat_interval_secs = springtale_runtime::operations::heartbeat::boot_interval(
        &*runtime.store,
        ctx.heartbeat_interval_secs,
    )
    .await;
    let springtale_runtime::EmbeddedBootHandle {
        scheduler: embedded_scheduler,
        heartbeat_monitor,
    } = springtale_runtime::bootstrap_embedded(&runtime, heartbeat_interval_secs)
        .await
        .map_err(|e| anyhow::anyhow!("scheduler bootstrap failed: {e}"))?;
    let trigger_tx = embedded_scheduler.trigger_tx.clone();

    // Daemon-side background tasks. Separate from `runtime.tasks`
    // because they belong to this shell, not to the shared runtime —
    // but a lock aborts both.
    let daemon_tasks = springtale_runtime::TaskHandles::new();

    // ── Initialize bot + connector gateways ──
    // Chat ingress lives on the runtime (plan 6.4): connector chat loops
    // are wired off the registry, so the daemon only takes the receiving
    // end for the bot and clones the sender for webhook ingress.
    let bot_msg_rx = runtime
        .take_chat_rx()
        .await
        .context("runtime chat receiver already taken")?;
    let api_bot_msg_tx = runtime.chat_tx.clone();
    // W5 in-app chat broadcast — created here so both the bot response
    // dispatcher (producer) and AppState's `GET /chat/stream` (consumers)
    // share the same channel.
    let (chat_tx, _chat_rx) = tokio::sync::broadcast::channel::<api::chat::ChatStreamMessage>(256);
    bot::init_bot(
        &runtime,
        embedded_scheduler.clone(),
        bot::BotChannels {
            bot_msg_rx,
            chat_tx: chat_tx.clone(),
        },
        formation_cmd_rx,
        formations_handle,
        &daemon_tasks,
    )
    .await?;

    // ── Restore formations persisted from a previous run ──
    // `init_bot` has already spawned the event loop that owns
    // `formation_cmd_rx`, so these sends queue behind it rather than
    // blocking boot (§6.11 / finding 119).
    let formations_restored =
        formations::restore_formations(&runtime.store, &formation_cmd_tx_for_restore).await?;
    tracing::info!(
        formations_restored,
        "formation restore step complete at boot"
    );

    // ── ConnectorEvent handlers are wired inside `bootstrap_embedded`
    // (shared with desktop), which publishes the registry on
    // `RuntimeState`. Clone it for AppState so the rule CRUD handlers
    // attach/detach through the same instance.
    let trigger_registry = runtime.trigger_registry.get().cloned().unwrap_or_else(|| {
        springtale_runtime::TriggerRegistry::new(trigger_tx.clone(), runtime.store.clone())
    });

    // ── Start data retention purge (if configured) ──
    if let Some(days) = ctx.store.retention_days {
        let purge_store = runtime.store.clone();
        daemon_tasks.spawn(async move {
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

    // ── Assemble the API state ──
    // Ready immediately: by the time this state exists every subsystem
    // above is up. A cold boot flips the flag again after binding; an
    // unlock has nothing left to wait for.
    let ready_flag = Arc::new(std::sync::atomic::AtomicBool::new(true));

    // Plan 6.7 — the runtime owns the events broadcast so runtime-side
    // announcers (approval gate) reach `GET /events/stream`.
    let event_tx = runtime.event_tx.clone();
    let (lock_signal, _lock_rx) = tokio::sync::watch::channel(false);

    let state = api::state::AppState {
        runtime,
        api_token_hash,
        sessions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        ready: ready_flag,
        trigger_tx,
        scheduler: embedded_scheduler,
        rate_limit_per_sec: u64::from(ctx.api.rate_limit_per_sec),
        event_tx,
        heartbeat_monitor,
        trigger_registry,
        bot_msg_tx: api_bot_msg_tx,
        chat_tx,
        stream_tickets: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        lock_signal,
    };

    Ok(Live::new(state, daemon_tasks, vault, Some(transport)))
}
