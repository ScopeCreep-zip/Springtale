//! Shared initialization — extracted from springtaled's boot sequence.
//!
//! These functions are the reusable core that both springtaled and
//! the desktop app call. No background tasks spawned here — that's
//! app-specific (daemon spawns scheduler/bot, desktop spawns Tauri).

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::OperationError;

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::RuleEngine;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::config::RuntimeConfig;
use crate::state::RuntimeState;

/// Initialize the full shared runtime.
///
/// Equivalent to springtaled's boot Steps 1-5b:
/// store → rules → connectors → AI adapter → sentinel.
///
/// Vault is NOT initialized here — desktop handles it via UI
/// (user types passphrase), springtaled reads from env/file.
pub async fn init(
    config: &RuntimeConfig,
    formation_cmd_tx: tokio::sync::mpsc::Sender<springtale_cooperation::command::FormationCommand>,
    live_formations: Option<Arc<dyn crate::state::LiveFormationReader>>,
    approval_gate: Option<Arc<dyn springtale_sentinel::ApprovalGate>>,
) -> Result<RuntimeState, OperationError> {
    // One runtime per store (plan 0.6): hold an exclusive flock on
    // `<db path>.lock` for the life of this `RuntimeState`. The kernel
    // drops it when the process dies, so there is no stale-heartbeat
    // window and nothing for shutdown to clean up. Ephemeral stores have
    // no path and no lock. The lock file lives beside the database, so
    // the data directory must exist first.
    let lock = if config.store.ephemeral {
        None
    } else {
        ensure_store_dir(&config.store.path)?;
        Some(Arc::new(acquire_runtime_lock(&config.store.path)?))
    };

    let store = init_store(&config.store).await?;
    tracing::info!("store initialized");

    let engine = init_engine(&store).await?;

    // Shared WASM engine — all WASM connectors use the same engine
    // so epoch interrupts work from a single ticker.
    let wasm_engine = Arc::new(
        springtale_connector::wasm::WasmEngine::new(
            springtale_connector::wasm::SandboxLimits::default(),
        )
        .map_err(|e| OperationError::Init(format!("WASM engine creation failed: {e}")))?,
    );

    // Shared per-tier `InstancePre` cache (§16). Every WASM connector's
    // module is pre-instantiated against all four tiers here so momentum
    // transitions are hash-lookup + `InstancePre::instantiate` with no
    // Linker rebuild. Also lives in `RuntimeState` so the forthcoming
    // `CapabilityBridge` can route tier transitions to specific hosts.
    let wasm_tier_cache = Arc::new(
        springtale_connector::wasm::WasmTierCache::new(wasm_engine.clone())
            .map_err(|e| OperationError::Init(format!("WASM tier cache init failed: {e}")))?,
    );

    let registry = init_registry(
        &store,
        &config.connector_configs,
        &wasm_engine,
        &wasm_tier_cache,
    )
    .await?;
    let ai_adapter_arc = init_adapter(&store).await?;
    // Plan 6.7 — the chat gate is built before the sentinel so a shell
    // that supplies no UI gate (springtaled, CLI) prompts through chat +
    // dashboard instead of sentinel-side default-deny. A caller-supplied
    // gate (desktop UI) still wins.
    let chat_gate = Arc::new(crate::approval::ChatApprovalGate::new(store.clone()));
    let sentinel_gate: Arc<dyn springtale_sentinel::ApprovalGate> = approval_gate
        .unwrap_or_else(|| Arc::new(crate::approval::SentinelChatGate::new(chat_gate.clone())));
    let sentinel = init_sentinel(config, &store, sentinel_gate);

    // Start WASM epoch ticker — increments every 1s so wall-clock
    // timeouts actually fire. Without this, a malicious WASM module
    // doing blocking I/O could run forever (fuel only counts instructions).
    {
        let ticker_engine = wasm_engine.engine().clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                ticker_engine.increment_epoch();
            }
        });
        tracing::info!("WASM epoch ticker started (1s interval)");
    }

    // Cooperation deposit sweeper — per COOPERATION.md §20.3: every 5s
    // delete `coop_deposits` rows whose `expires_at` is past. Without
    // this, abandoned environment-mediated handoffs accumulate
    // indefinitely because exactly-once collection removes claimed
    // rows but never expired ones.
    {
        let store_sweeper = store.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                match store_sweeper.coop_sweep_expired().await {
                    Ok(n) if n > 0 => {
                        tracing::debug!(swept = n, "cooperation deposit sweeper");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "deposit sweep failed");
                    }
                }
            }
        });
        tracing::info!("cooperation deposit sweeper started (5s interval)");
    }

    // Phase B.4 executions-log vacuum — every hour delete rows
    // past their `retention_until`. Default retention is 14 days
    // (see `operations::executions::recorder::DEFAULT_RETENTION_MS`).
    // Cascades to `execution_steps` via the FK ON DELETE CASCADE.
    {
        let store_sweeper = store.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                let now_ms = chrono::Utc::now().timestamp_millis();
                match store_sweeper.vacuum_executions(now_ms).await {
                    Ok(n) if n > 0 => {
                        tracing::debug!(purged = n, "executions vacuum");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "executions vacuum failed");
                    }
                }
            }
        });
        tracing::info!("executions vacuum started (1h interval)");
    }

    // Gossip substrate (§8). Selected by `CooperationConfig::cross_process`:
    // single-process deployments get `InMemoryGossipStore` (DashMap);
    // cross-process deployments spawn `ChitchatGossipStore` over UDP.
    let gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore> =
        init_gossip_store(&config.cooperation).await?;

    // SWIM liveness (§8.3). Only spawned when `cross_process = true` —
    // single-process deployments have no peer processes to probe. The
    // node also drives peer-scoped gossip sweeping: when SWIM declares
    // a peer dead, `MemberDown` fires and `GossipStore::remove_by_peer`
    // drops every entry the peer published, so `snapshots()` no longer
    // surfaces stale data. The identifier the sweep uses is the peer's
    // SocketAddr string — the same id chitchat stamps onto incoming
    // entries via `GossipEntry::with_peer_id`.
    let swim_node = init_swim_node(&config.cooperation, gossip_store.clone()).await?;

    // Canvas/A2UI
    let canvas = Arc::new(tokio::sync::RwLock::new(
        springtale_core::canvas::CanvasState::default(),
    ));
    let (canvas_tx, _) = tokio::sync::broadcast::channel(64);

    // Delivery fan-out for fired Notify/SendMessage steps. Capacity
    // 256 — a deployed colony fires far fewer user-facing
    // notifications than canvas ticks, and subscribers (chat SSE / OS
    // notification) drain promptly. Lagged receivers drop the oldest
    // (broadcast semantics) — acceptable for best-effort delivery.
    let (notification_tx, _) = tokio::sync::broadcast::channel(256);
    // Events broadcast behind `GET /events/stream` / desktop `event-fired`.
    // Owned here (plan 6.7) so runtime-side announcers reach it.
    let (event_tx, _) =
        tokio::sync::broadcast::channel::<springtale_store::schema::events::EventEntry>(256);

    // W3.B — canvas state syncer. Subscribes to `canvas_tx` and
    // applies every broadcast update to the in-memory `canvas` state
    // so callers of `operations::canvas::get_canvas` see the latest
    // snapshot. Previously this dual-write happened inside the
    // `update_canvas` IPC command; the tick step at
    // `bot::runtime::tick_steps::emit_canvas_update` only
    // broadcasts. Without this task the snapshot diverges from the
    // broadcast stream after the first tick.
    {
        let canvas = canvas.clone();
        let mut rx = canvas_tx.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(update) => {
                        canvas.write().await.apply(&update);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "canvas syncer lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Phase H2 — cooperation events bus. Capacity 512 covers ~30s of
    // headroom at 4 formations × 30Hz × ~5 events/tick. Lagged readers
    // drop silently per the events_stream.rs precedent.
    let (cooperation_tx, _) = tokio::sync::broadcast::channel(512);
    // Plan §1.15 F/G: utterance ring + def table + the bot-written tick clock.
    let utterances = crate::utterance_ring::UtteranceRing::default();
    crate::utterance_ring::spawn_collector(utterances.clone(), cooperation_tx.subscribe());
    let utterance_defs = Arc::new(config.cooperation.utterances.clone());
    let cadence_tick = Arc::new(std::sync::atomic::AtomicU64::new(0));
    // G6 — cross-formation gossip bus. In-memory default; cross-process
    // deployments can swap in a chitchat-backed impl in a follow-up.
    let formation_gossip: Arc<dyn springtale_cooperation::gossip::FormationGossipBus> =
        springtale_cooperation::gossip::InMemoryFormationGossipBus::new();
    // G2 — global cross-formation knowledge store. SQLite-backed so
    // outcomes survive process restart (encrypted-at-rest via the vault
    // layer that wraps `config_store`). A future Qdrant Edge backend can
    // land behind the same `GlobalKnowledgeStore` trait.
    let knowledge_store: Arc<dyn springtale_cooperation::memory::GlobalKnowledgeStore> =
        springtale_cooperation::memory::PersistentKnowledgeStore::new(store.clone());

    // Capability bridge (Phase 17) — binds the registry to a per-invocation
    // tier dispatch path. See `crate::cooperation::capability_bridge`.
    //
    // Phase 0: share the same `Arc<ArcSwap<...>>` `RuntimeState` holds so
    // dispatcher AiComplete arms resolve the live adapter through
    // `bridge.ai_adapter_for(...)`. Wrap once here so both the state field
    // and the bridge share the inner ArcSwap.
    let ai_adapter_handle: Arc<arc_swap::ArcSwap<Arc<dyn springtale_ai::AiAdapter>>> =
        Arc::new(arc_swap::ArcSwap::from(Arc::new(ai_adapter_arc)));
    let executions_recorder: Arc<dyn crate::operations::executions::ExecutionRecorder> = Arc::new(
        crate::operations::executions::StoreRecorder::new(store.clone()),
    );
    // OpenClaw CVE-2026-25253 1-click-RCE class: every connector that
    // declared `Capability::ShellExec` routes invocations through a
    // blocking approval gate before reaching `host.execute_checked`.
    // W2: the gate is now the store-backed [`crate::approval::ChatApprovalGate`]
    // — pending approvals are durable rows (restart-safe, deny-by-default
    // on expiry) and each new request is announced on a broadcast channel
    // the bot-side notifier turns into a 3-button chat card. Single Arc
    // shared across RuntimeState + bridge so `POST /approvals/:id` AND the
    // chat callback path resolve into the same gate the dispatcher awaits.
    // Boot sweep — 2026 durable-resume semantics (LangGraph thread pattern +
    // OWASP Agentic bind+expiry): pending approvals SURVIVE a restart; only
    // rows past their `expires_at` are denied here. Safety comes from the
    // approval being bound to the exact persisted action with an expiry and
    // single-use resolve — not from killing it on process death. Expired
    // rows also drop their conversation checkpoints (dead threads).
    match store
        .expire_pending_approvals(chrono::Utc::now().timestamp_millis())
        .await
    {
        Ok(expired) if !expired.is_empty() => {
            tracing::info!(
                count = expired.len(),
                "boot sweep denied EXPIRED pending approvals"
            );
            for row in &expired {
                if let Ok(Some(cp)) = store.get_checkpoint_by_approval(&row.id).await {
                    let _ = store.delete_tool_loop_checkpoint(&cp.session_key).await;
                }
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "pending-approval boot sweep failed"),
    }
    let approval_gate: Arc<dyn crate::approval::ApprovalGate> = chat_gate.clone();

    // OWASP LLM10 (Unbounded Consumption) — per-bot daily token
    // quota persisted to SQLite (Phase-7 audit Finding D). Replaces
    // the in-process `InMemoryTokenQuota` so daemon restart no
    // longer resets the counters. The optional daily cap is sourced
    // from `RuntimeConfig.sentinel` — `None` means observability
    // mode (records usage, never denies).
    let daily_token_limit = config.sentinel.as_ref().and_then(|s| s.daily_token_limit);
    let token_quota: Arc<dyn springtale_ai::TokenQuota> = Arc::new(
        crate::quota::SqliteTokenQuota::new(store.clone(), daily_token_limit),
    );
    let ai_guardrails = crate::cooperation::capability_bridge::AiGuardrailHandles {
        quota: token_quota,
        refusal_counter: springtale_ai::RefusalCounter::default(),
        output_cap_bytes: springtale_ai::DEFAULT_OUTPUT_CAP_BYTES,
    };

    let capability_bridge = crate::cooperation::CapabilityBridge::new(registry.clone())
        .with_ai_adapter(ai_adapter_handle.clone())
        .with_store(store.clone())
        .with_recorder(executions_recorder)
        .with_approval_gate(approval_gate)
        .with_ai_guardrails(ai_guardrails);

    // Approval announcer (plan 6.7). Every gate announcement is published
    // on the events stream first (`trigger_type = "approval_required"`) so
    // whoever is watching a dashboard sees it; then, when the request
    // carries a chat origin, a 3-button card goes to that channel.
    // Telegram gets a real inline keyboard; other connectors get a typed
    // `apr:<id>:y|n` reply fallback. Exactly three actions (Nintendo rule).
    // Rule / formation fires have no origin — dashboard only.
    {
        let mut announcements = chat_gate.subscribe();
        let bridge = capability_bridge.clone();
        let announce_tx = event_tx.clone();
        tokio::spawn(async move {
            while let Ok(req) = announcements.recv().await {
                let expires_at = req.requested_at
                    + chrono::Duration::from_std(crate::approval::CHAT_APPROVAL_TIMEOUT)
                        .unwrap_or_else(|_| chrono::Duration::seconds(900));
                let _ = announce_tx.send(springtale_store::schema::events::EventEntry {
                    id: uuid::Uuid::new_v4(),
                    connector_name: req.connector_name.clone(),
                    trigger_type: "approval_required".to_owned(),
                    timestamp: chrono::Utc::now(),
                    action_taken: serde_json::json!({
                        "id": req.id.to_string(),
                        "summary": req.summary,
                        "expires_at": expires_at.to_rfc3339(),
                    })
                    .to_string(),
                });
                let Some(origin) = req.origin.as_ref() else {
                    tracing::info!(approval = %req.id, "approval has no chat origin — dashboard only; deny-on-timeout applies");
                    continue;
                };
                let conn = origin.connector.as_str();
                let chan = origin.channel_id.as_str();
                let text = format!(
                    "⚠️ Approval needed\n{} wants to run:\n{}",
                    req.connector_name, req.summary
                );
                let (action, input) = if conn == "connector-telegram" {
                    (
                        "send_inline_keyboard",
                        serde_json::json!({
                            "chat_id": chan,
                            "text": text,
                            "inline_keyboard": [
                                [{"text": "✅ Approve", "callback_data": format!("apr:{}:y", req.id)}],
                                [{"text": "❌ Deny", "callback_data": format!("apr:{}:n", req.id)}],
                                [{"text": "👁 Details", "callback_data": format!("apr:{}:d", req.id)}],
                            ],
                        }),
                    )
                } else {
                    (
                        "send_message",
                        serde_json::json!({
                            "channel_id": chan,
                            "chat_id": chan,
                            "text": format!("{text}\nReply `apr:{}:y` to approve or `apr:{}:n` to deny.", req.id, req.id),
                        }),
                    )
                };
                if let Err(e) = bridge
                    .execute(
                        conn,
                        action,
                        input,
                        springtale_connector::tier::WasmTier::Warming,
                    )
                    .await
                {
                    tracing::warn!(approval = %req.id, error = %e, "approval card delivery failed");
                }
            }
        });
    }

    // Role registry (Phase 21) — starts with built-in General/Information/
    // Support. Community roles are folded in by `register_manifest_roles`
    // during connector install, and by the same helper applied to any
    // manifests that were loaded from the store in `init_registry`.
    let role_registry = Arc::new(springtale_cooperation::role::RoleRegistry::with_builtins());
    register_persisted_manifest_roles(&store, &role_registry).await;

    Ok(RuntimeState {
        store,
        registry,
        engine,
        ai_adapter: ai_adapter_handle,
        sentinel,
        wasm_engine,
        wasm_tier_cache,
        capability_bridge,
        role_registry,
        canvas,
        canvas_tx,
        notification_tx,
        event_tx,
        trigger_registry: Arc::new(std::sync::OnceLock::new()),
        cooperation_tx,
        utterances,
        utterance_defs,
        cadence_tick,
        formation_cmd_tx,
        live_formations,
        gossip_store,
        formation_gossip,
        knowledge_store,
        swim_node,
        _lock: lock,
    })
}

/// Walk all connector manifests already in the store and fold their
/// `roles` declarations into the shared `RoleRegistry`. Called once at
/// init. Future connector installs register directly via
/// `operations::connectors::install` (Phase 21 wiring).
async fn register_persisted_manifest_roles(
    store: &Arc<dyn springtale_store::StorageBackend>,
    role_registry: &Arc<springtale_cooperation::role::RoleRegistry>,
) {
    let Ok(rows) = store.list_connectors().await else {
        return;
    };
    for row in rows {
        let Ok(manifest) =
            serde_json::from_str::<springtale_connector::ConnectorManifest>(&row.manifest_json)
        else {
            tracing::warn!(
                connector = %row.name,
                "manifest JSON invalid — skipping role registration"
            );
            continue;
        };
        crate::cooperation::register_manifest_roles(role_registry, &manifest);
    }
}

/// Construct the SWIM liveness node when cross-process mode is on.
///
/// Also wires an event bridge from the node's broadcast channel into
/// the shared gossip store: `MemberDown` → `GossipStore::remove`. That
/// way awareness snapshots (`GossipStore::snapshots`) stop surfacing
/// stale entries for peers the SWIM protocol has given up on.
async fn init_swim_node(
    cfg: &crate::config::CooperationConfig,
    gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore>,
) -> Result<Option<Arc<springtale_cooperation::awareness::SwimNode>>, OperationError> {
    use springtale_cooperation::awareness::{SwimNode, SwimNodeConfig};
    use std::num::NonZeroU32;

    if !cfg.cross_process {
        return Ok(None);
    }

    let listen: std::net::SocketAddr = cfg
        .swim_listen_addr
        .as_deref()
        .unwrap_or("127.0.0.1:0")
        .parse()
        .map_err(|e| OperationError::Init(format!("invalid swim_listen_addr: {e}")))?;

    let seeds: Vec<std::net::SocketAddr> = cfg
        .swim_seeds
        .iter()
        .map(|s| {
            s.parse()
                .map_err(|e| OperationError::Init(format!("invalid swim seed {s}: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let swim_cfg = SwimNodeConfig {
        listen,
        seeds,
        cluster_size: NonZeroU32::new(10).unwrap_or(NonZeroU32::MIN),
    };
    let node = SwimNode::spawn(swim_cfg)
        .await
        .map_err(|e| OperationError::Init(format!("swim spawn: {e}")))?;
    tracing::info!(
        addr = %node.local_addr(),
        seeds = cfg.swim_seeds.len(),
        "SWIM liveness node started"
    );

    // Spawn a dual-purpose consumer: (a) audit-log every SwimEvent so
    // peer liveness transitions are observable, and (b) on MemberDown
    // sweep the shared gossip store of any peer-owned snapshots so
    // awareness reads stop seeing defunct peers. This is the live
    // consumer that gives the SWIM node a real job beyond logs.
    //
    // Sweep strategy: entries received from remote peers carry
    // `GossipEntry::peer_id = Some(peer.to_string())` (stamped by the
    // chitchat adapter when decoding). `GossipStore::remove_by_peer`
    // filters on that field, so a peer going down cleanly drops every
    // entry the peer owned. Locally-published entries have
    // `peer_id = None` and are never swept by this path.
    {
        use springtale_cooperation::awareness::{SwimEvent, SwimSelfState};
        let mut rx = node.subscribe();
        let sweep_gossip = gossip_store.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(SwimEvent::MemberUp(peer)) => {
                        tracing::info!(peer = %peer, "SWIM: peer up");
                    }
                    Ok(SwimEvent::MemberDown(peer)) => {
                        let peer_id = peer.to_string();
                        let removed = sweep_gossip.remove_by_peer(&peer_id).await;
                        tracing::warn!(
                            peer = %peer,
                            gossip_entries_swept = removed,
                            "SWIM: peer down"
                        );
                    }
                    Ok(SwimEvent::MemberRejoined(peer)) => {
                        tracing::info!(peer = %peer, "SWIM: peer rejoined");
                    }
                    Ok(SwimEvent::SelfState(SwimSelfState::Defunct)) => {
                        tracing::error!("SWIM: self declared defunct");
                    }
                    Ok(SwimEvent::SelfState(state)) => {
                        tracing::debug!(?state, "SWIM: self state");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "SWIM event consumer lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    Ok(Some(Arc::new(node)))
}

/// Construct the gossip substrate based on cooperation config.
async fn init_gossip_store(
    cfg: &crate::config::CooperationConfig,
) -> Result<Arc<dyn springtale_cooperation::awareness::GossipStore>, OperationError> {
    use springtale_cooperation::awareness::{
        ChitchatGossipConfig, ChitchatGossipStore, InMemoryGossipStore,
    };

    if !cfg.cross_process {
        tracing::info!("gossip: in-memory (single-process)");
        return Ok(Arc::new(InMemoryGossipStore::new()));
    }

    let listen_str = cfg.chitchat_listen_addr.as_deref().ok_or_else(|| {
        OperationError::Init(
            "cooperation.cross_process = true requires chitchat_listen_addr".into(),
        )
    })?;
    let listen: std::net::SocketAddr = listen_str
        .parse()
        .map_err(|e| OperationError::Init(format!("invalid chitchat_listen_addr: {e}")))?;

    let chitchat_cfg = ChitchatGossipConfig {
        node_id: format!("springtale-{}", uuid::Uuid::new_v4()),
        listen_addr: listen,
        public_addr: listen,
        seeds: cfg.chitchat_seeds.clone(),
        cluster_id: cfg.cluster_id.clone(),
        gossip_interval: std::time::Duration::from_secs(1),
    };
    tracing::info!(
        addr = %listen,
        seeds = cfg.chitchat_seeds.len(),
        cluster = %cfg.cluster_id,
        "gossip: chitchat (cross-process)"
    );
    let store = ChitchatGossipStore::spawn(chitchat_cfg)
        .await
        .map_err(|e| OperationError::Init(format!("chitchat spawn: {e}")))?;
    Ok(Arc::new(store))
}

/// Initialize the store backend.
async fn init_store(
    config: &crate::config::StoreConfig,
) -> Result<Arc<dyn springtale_store::StorageBackend>, OperationError> {
    if config.ephemeral {
        tracing::warn!("EPHEMERAL MODE — all state in memory, lost on exit");
        Ok(Arc::new(springtale_store::backend::InMemoryBackend::new()))
    } else {
        // No plaintext mode (plan 0.5): a store is encrypted or it is
        // in memory. A config that lost its key fails here, loudly,
        // instead of silently writing an unencrypted file to disk.
        let backend = match config.encryption_key_hex.as_deref() {
            Some(key) => {
                tracing::info!(path = %config.path.display(), "opening encrypted SQLite store");
                ensure_store_dir(&config.path)?;
                SqliteBackend::open_encrypted(&config.path, key).map_err(|e| {
                    OperationError::Init(format!("failed to open encrypted store: {e}"))
                })?
            }
            None => {
                return Err(OperationError::Init(
                    "store.encryption_key_hex is required; run `springtale init` or set \
                     store.ephemeral = true for a throwaway in-memory store"
                        .into(),
                ));
            }
        };
        Ok(Arc::new(backend))
    }
}

/// Create the store's parent directory if it does not exist yet.
fn ensure_store_dir(db_path: &Path) -> Result<(), OperationError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            OperationError::Init(format!(
                "failed to create data directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    Ok(())
}

/// Take the exclusive runtime lock on `<db_path>.lock` (plan 0.6).
///
/// flock(2) is advisory and released when every descriptor on the file
/// is closed — the kernel does that when the process dies — so a crash
/// can never leave a stale lock. `WouldBlock` means another runtime
/// holds it. The `RwLock` is leaked on purpose: the guard lives inside
/// `RuntimeState` for the life of the process and needs `'static`.
fn acquire_runtime_lock(
    db_path: &Path,
) -> Result<fd_lock::RwLockWriteGuard<'static, File>, OperationError> {
    let path = db_path.with_extension("lock");
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts
        .open(&path)
        .map_err(|e| OperationError::Init(format!("cannot open {}: {e}", path.display())))?;
    // Process-lifetime lock: the RwLock lives as long as the process, so the guard can be 'static.
    let lock: &'static mut fd_lock::RwLock<File> = Box::leak(Box::new(fd_lock::RwLock::new(file)));
    lock.try_write().map_err(|e| match e.kind() {
        std::io::ErrorKind::WouldBlock => OperationError::Init(format!(
            "another Springtale runtime holds {}; stop it first",
            path.display()
        )),
        _ => OperationError::Init(format!("cannot lock {}: {e}", path.display())),
    })
}

/// Load rules from store into a RuleEngine.
async fn init_engine(
    store: &Arc<dyn springtale_store::StorageBackend>,
) -> Result<Arc<RwLock<RuleEngine>>, OperationError> {
    let rules = store
        .list_rules()
        .await
        .map_err(|e| OperationError::Init(format!("failed to load rules: {e}")))?;

    let mut engine = RuleEngine::new();
    let mut loaded = 0;
    for rule in &rules {
        if let Err(e) = engine.add_rule(rule.clone()) {
            tracing::warn!(rule = %rule.name, error = %e, "skipping invalid rule");
        } else {
            loaded += 1;
        }
    }
    tracing::info!(total = rules.len(), loaded, "rule engine loaded");

    Ok(Arc::new(RwLock::new(engine)))
}

/// Discover compiled-in connectors via `inventory` and instantiate those
/// whose config sections are present.
async fn init_registry(
    _store: &Arc<dyn springtale_store::StorageBackend>,
    connector_configs: &std::collections::HashMap<String, serde_json::Value>,
    shared_wasm_engine: &Arc<springtale_connector::wasm::WasmEngine>,
    shared_wasm_tier_cache: &Arc<springtale_connector::wasm::WasmTierCache>,
) -> Result<Arc<RwLock<ConnectorRegistry>>, OperationError> {
    use springtale_connector::factory::FactoryEntry;

    let mut registry = ConnectorRegistry::new(CapabilityPolicy::Interactive);
    let mut registered = 0u32;

    // Load the set of connectors explicitly removed by the user.
    // Prevents auto-loading of no-config connectors (shell, filesystem)
    // that were removed via the UI.
    let stored_config = _store.list_config().await.unwrap_or_default();
    let removed_connectors: std::collections::HashSet<String> = stored_config
        .iter()
        .filter_map(|(k, _)| k.strip_prefix("connector-removed:").map(|s| s.to_owned()))
        .collect();

    // Config keys the user configured via the UI (`setup_connector`
    // writes `connector:{config_key}`). A name is registered once, so
    // the first pass to install wins: the no-config default install
    // below must yield to these so the config-store pass installs the
    // UI-configured instance, as it did when installs overwrote.
    let ui_configured_keys: std::collections::HashSet<String> = stored_config
        .iter()
        .filter_map(|(k, _)| k.strip_prefix("connector:").map(|s| s.to_owned()))
        .collect();

    // No-config connectors (shell, filesystem) auto-load whenever the user
    // has not explicitly configured or removed them. They are not a privacy
    // risk on first run: both are fenced by their own capability allow-lists
    // and go through the sentinel like any other connector action, so
    // loading them by default does not widen what a fresh vault can do.

    for entry in inventory::iter::<FactoryEntry> {
        let factory = entry.factory;
        let key = factory.config_key();

        // Skip connectors explicitly removed by the user
        if removed_connectors.contains(factory.name()) {
            tracing::debug!(
                connector = factory.name(),
                "skipping — explicitly removed by user"
            );
            continue;
        }

        if let Some(config_value) = connector_configs.get(key) {
            match factory.create(config_value.clone()).await {
                Ok(connector) => match registry.install_native(connector) {
                    Ok(name) => {
                        tracing::info!(connector = %name, "auto-registered connector");
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            connector = factory.name(),
                            error = %e,
                            "failed to install connector"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        connector = factory.name(),
                        error = %e,
                        "failed to instantiate connector, skipping"
                    );
                }
            }
        } else if !factory.requires_config() && !ui_configured_keys.contains(key) {
            match factory
                .create(serde_json::Value::Object(Default::default()))
                .await
            {
                Ok(connector) => match registry.install_native(connector) {
                    Ok(name) => {
                        tracing::info!(connector = %name, "auto-registered (no config needed)");
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            connector = factory.name(),
                            error = %e,
                            "failed to install connector"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        connector = factory.name(),
                        error = %e,
                        "failed to instantiate default connector, skipping"
                    );
                }
            }
        } else {
            tracing::debug!(
                connector = factory.name(),
                config_key = key,
                "no config found, not loading"
            );
        }
    }

    // Also load connectors configured via UI (stored in config_store as "connector:{key}").
    // TOML configs take precedence — config store only loads connectors not already loaded.
    // This is the counterpart to setup_connector() which writes to config_store.
    let loaded_keys: Vec<String> = inventory::iter::<FactoryEntry>
        .into_iter()
        .filter(|e| connector_configs.contains_key(e.factory.config_key()))
        .map(|e| e.factory.config_key().to_owned())
        .collect();

    if let Ok(stored) = _store.list_config().await {
        for (key, value_json) in &stored {
            let Some(config_key) = key.strip_prefix("connector:") else {
                continue;
            };
            if loaded_keys.contains(&config_key.to_owned()) {
                continue; // already loaded from TOML
            }
            let Ok(config_value) = serde_json::from_str::<serde_json::Value>(value_json) else {
                continue;
            };
            for entry in inventory::iter::<FactoryEntry> {
                if entry.factory.config_key() == config_key {
                    match entry.factory.create(config_value.clone()).await {
                        Ok(connector) => match registry.install_native(connector) {
                            Ok(name) => {
                                tracing::info!(connector = %name, "loaded from config store");
                                registered += 1;
                            }
                            Err(e) => tracing::warn!(
                                connector = entry.factory.name(),
                                error = %e,
                                "failed to install connector from config store"
                            ),
                        },
                        Err(e) => tracing::warn!(
                            connector = entry.factory.name(),
                            error = %e,
                            "failed to create connector from config store"
                        ),
                    }
                    break;
                }
            }
        }
    }

    // Load persisted WASM connectors from store (installed via UI/CLI).
    // These are community connectors that were installed as .wasm packages
    // and persisted in the wasm_binaries table.
    //
    // Phase-7 audit Finding #1 (R-004 hash re-check on every load):
    // before handing a persisted WASM binary to `install_wasm`, we
    // re-verify (a) the WASM bytes' SHA-256 matches the manifest
    // declaration, and (b) the manifest's Ed25519 signature against
    // the install-time pubkey pinned in `author_pubkey_hex`. The
    // store-pinned pubkey is the trust anchor (TUF §4) — it lives
    // outside the signed metadata, so an attacker who swaps the
    // manifest_json blob can't also forge a matching signature
    // without compromising the original signing key.
    {
        use springtale_connector::wasm::SandboxLimits;

        let wasm_binaries = _store.list_wasm_binaries().await.unwrap_or_default();
        if !wasm_binaries.is_empty() {
            for bin in wasm_binaries {
                if removed_connectors.contains(&bin.name) {
                    tracing::debug!(connector = %bin.name, "skipping removed WASM connector");
                    continue;
                }
                let manifest: springtale_connector::ConnectorManifest = match serde_json::from_str(
                    &bin.manifest_json,
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(connector = %bin.name, error = %e, "invalid WASM manifest JSON");
                        continue;
                    }
                };

                if let Err(e) = reverify_persisted_wasm(
                    &bin.name,
                    &bin.wasm_bytes,
                    &bin.wasm_hash,
                    &manifest,
                    &bin.author_pubkey_hex,
                    &bin.manifest_sig_hex,
                ) {
                    tracing::error!(
                        connector = %bin.name,
                        error = %e,
                        "PERSISTED WASM CONNECTOR FAILED RE-VERIFICATION — refusing to load"
                    );
                    continue;
                }

                match registry.install_wasm(
                    shared_wasm_engine.clone(),
                    &bin.wasm_bytes,
                    manifest,
                    SandboxLimits::default(),
                    shared_wasm_tier_cache.clone(),
                ) {
                    Ok(name) => {
                        tracing::info!(connector = %name, "loaded WASM connector from store");
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::warn!(connector = %bin.name, error = %e, "failed to load WASM connector");
                    }
                }
            }
        }
    }

    tracing::info!(registered, "connector registry initialized");
    Ok(Arc::new(RwLock::new(registry)))
}

/// Build the colony AI adapter from the store's `ai:colony` row. A
/// missing row builds `NoopAdapter` — the platform runs without AI.
/// A stored row that no longer builds (the provider went away after
/// `configure_ai_adapter` validated it) also falls back to `NoopAdapter`
/// so boot never depends on an AI provider.
async fn init_adapter(
    store: &Arc<dyn springtale_store::StorageBackend>,
) -> Result<Arc<dyn springtale_ai::AiAdapter>, OperationError> {
    let cfg = crate::operations::config::get_config(
        store.as_ref(),
        crate::operations::config::AI_COLONY_KEY,
    )
    .await?;
    match crate::operations::config::build_adapter(&cfg).await {
        Ok(adapter) => Ok(adapter),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "ai:colony config could not be built; running without AI until it is reconfigured"
            );
            crate::operations::config::build_adapter(&serde_json::Value::Null).await
        }
    }
}

/// Initialize the sentinel behavioral monitor.
///
/// W1.F — the optional `approval_gate` lets the application shell
/// (desktop / dashboard / future surfaces) supply a UI-backed
/// `ChannelApprovalGate` so destructive actions prompt the user
/// instead of silently denying. Headless shells get the
/// `SentinelChatGate` adapter (plan 6.7) — chat card + dashboard — so
/// someone is always asked; deny-on-timeout stays the safe default.
fn init_sentinel(
    config: &RuntimeConfig,
    store: &Arc<dyn springtale_store::StorageBackend>,
    gate: Arc<dyn springtale_sentinel::ApprovalGate>,
) -> Arc<springtale_sentinel::Sentinel> {
    let sentinel_config = config.sentinel.clone().unwrap_or_default();
    Arc::new(springtale_sentinel::Sentinel::with_approval_gate(
        sentinel_config,
        store.clone(),
        gate,
    ))
}

/// Phase-7 audit Finding #1 — re-verify a persisted WASM connector
/// before handing it to `registry.install_wasm` at daemon boot.
///
/// Checks:
///   1. WASM SHA-256 matches the stored `wasm_hash` AND the manifest's
///      declared `wasm_hash`. Defeats a swap of the WASM bytes alone.
///   2. Manifest signature (if pinned) verifies against the
///      `author_pubkey_hex` STORED at install time (not embedded in
///      the manifest). Defeats a swap-the-whole-bundle attack where
///      the attacker rewrites manifest + signature together: the
///      attacker would need to also overwrite the pinned pubkey, but
///      then their forged signature would have to verify against the
///      ORIGINAL signing key — which they don't have. TUF §4
///      trust-anchor separation.
///
/// Legacy rows (pre-v8 migration) carry empty `author_pubkey_hex`;
/// these are treated as "TOFU-grandfathered" and logged at WARN. We
/// don't fail closed on them so an existing deployment isn't bricked
/// by the audit fix; the operator sees the warning and can re-install
/// the connector to repopulate the pin.
fn reverify_persisted_wasm(
    name: &str,
    wasm_bytes: &[u8],
    expected_hash: &str,
    manifest: &springtale_connector::ConnectorManifest,
    pinned_pubkey_hex: &str,
    pinned_sig_hex: &str,
) -> Result<(), OperationError> {
    // 1. WASM hash re-check (R-004 "hash re-check on every load").
    springtale_connector::wasm::WasmEngine::verify_wasm_hash(wasm_bytes, expected_hash).map_err(
        |e| OperationError::Validation(format!("wasm_hash re-verification failed for {name}: {e}")),
    )?;

    // Also require the manifest's declared wasm_hash matches what
    // we stored — defends against a swap where the attacker keeps
    // the WASM bytes intact but rewrites the manifest to remove or
    // change the declared hash.
    let manifest_hash = manifest.wasm_hash.as_deref().unwrap_or("");
    if manifest_hash != expected_hash {
        return Err(OperationError::Validation(format!(
            "manifest wasm_hash drift for {name}: stored {expected_hash}, manifest {manifest_hash}"
        )));
    }

    // 2. Signature re-verify against the pinned trust anchor.
    if pinned_pubkey_hex.is_empty() || pinned_sig_hex.is_empty() {
        // Legacy install (pre-v8) — log + accept. Operators re-install
        // to upgrade the row to a pinned-pubkey one.
        tracing::warn!(
            connector = %name,
            "WASM connector loaded without pinned author pubkey — \
             legacy pre-v8 install. Re-install to enable boot-time \
             signature re-verification (Phase-7 audit Finding #1)."
        );
        return Ok(());
    }

    let pubkey_bytes = hex::decode(pinned_pubkey_hex).map_err(|e| {
        OperationError::Validation(format!(
            "pinned author pubkey for {name} is invalid hex: {e}"
        ))
    })?;
    let pubkey_arr: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
        OperationError::Validation(format!("pinned author pubkey for {name} must be 32 bytes"))
    })?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_arr).map_err(|e| {
        OperationError::Validation(format!(
            "pinned author pubkey for {name} not a valid Ed25519 key: {e}"
        ))
    })?;

    // The manifest's `signature` field must equal the install-time
    // pinned signature — and that signature must verify against the
    // pinned pubkey. We require BOTH so a tampered manifest_json
    // (with a different but valid signature) is still rejected.
    let manifest_sig = manifest.signature.as_deref().unwrap_or("");
    if manifest_sig != pinned_sig_hex {
        return Err(OperationError::Validation(format!(
            "manifest signature drift for {name}: stored sig does not match manifest sig"
        )));
    }

    springtale_connector::manifest::verify::verify_manifest_signature(manifest, &verifying_key)
        .map_err(|e| {
            OperationError::Validation(format!("signature re-verification failed for {name}: {e}"))
        })?;

    tracing::debug!(
        connector = %name,
        "WASM connector trust-anchor re-verified at boot"
    );
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_runtime_lock_second_holder_rejected_until_first_drops() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("springtale.db");
        let lock_path = db.with_extension("lock").display().to_string();

        let first = acquire_runtime_lock(&db).expect("first runtime takes the lock");

        match acquire_runtime_lock(&db) {
            Err(OperationError::Init(msg)) => {
                assert!(
                    msg.contains(&lock_path),
                    "Init message names the lock path: {msg}"
                );
            }
            Err(other) => panic!("expected OperationError::Init, got {other:?}"),
            Ok(_) => panic!("second runtime acquired the lock while the first still held it"),
        }

        drop(first);
        let third = acquire_runtime_lock(&db).expect("lock free again after first guard drops");
        drop(third);
    }

    #[tokio::test]
    async fn test_init_store_no_key_not_ephemeral_returns_init_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("springtale.db");
        let config = crate::config::StoreConfig {
            path: db.clone(),
            ephemeral: false,
            encryption_key_hex: None,
            ..Default::default()
        };

        match init_store(&config).await {
            Err(OperationError::Init(msg)) => {
                assert!(
                    msg.contains("encryption_key_hex"),
                    "Init message names the key: {msg}"
                );
            }
            Err(other) => panic!("expected OperationError::Init, got {other:?}"),
            Ok(_) => panic!("plaintext store opened without a key"),
        }
        assert!(!db.exists(), "no plaintext database file was created");
    }
}
