//! In-memory `AppState` + router, the single copy both test suites use.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axum::Router;
use tokio::sync::{Mutex, RwLock, mpsc};

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::command::FormationCommand;
use springtale_core::rule::engine::RuleEngine;
use springtale_crypto::token::derive_api_token_hash;
use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::api::build_router;
use crate::api::state::AppState;

/// A built router plus everything a test needs to talk to it.
pub struct TestApp {
    /// The real management API router.
    pub router: Router,
    /// Hex-encoded API token accepted as `Authorization: Bearer …`.
    pub token_hex: String,
    /// Receiver for formation commands the API enqueues. Kept alive by
    /// the caller — dropping it makes `formation_cmd_tx.send` fail.
    pub formation_cmd_rx: mpsc::Receiver<FormationCommand>,
    /// The same state the router was built over — lets a test reach the
    /// registry, the sentinel and the capability bridge directly.
    pub state: AppState,
}

impl TestApp {
    /// Build a router over an in-memory store.
    ///
    /// `ready` seeds the readiness flag: pass `false` for tests that need
    /// the daemon to appear as still booting. Must be called from inside
    /// a Tokio runtime (the fs watcher registers with it).
    pub fn build(ready: bool) -> Self {
        let store: Arc<dyn springtale_store::StorageBackend> =
            Arc::new(SqliteBackend::open_in_memory().unwrap());
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::AllowAll,
        )));
        let engine = Arc::new(RwLock::new(RuleEngine::new()));

        let passphrase = b"test-passphrase";
        let api_token_hash = derive_api_token_hash(passphrase);
        let token_hex = hex::encode(api_token_hash);

        let (trigger_tx, _trigger_rx) = mpsc::channel(256);
        let cron = Arc::new(Mutex::new(CronExecutor::new(trigger_tx.clone())));
        let fs_watcher = Arc::new(Mutex::new(FsWatcher::new(trigger_tx.clone()).unwrap()));

        let ready_flag = Arc::new(AtomicBool::new(ready));

        let sentinel = Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));

        let ai_adapter = Arc::new(arc_swap::ArcSwap::from(Arc::new(Arc::new(
            springtale_ai::NoopAdapter,
        )
            as Arc<dyn springtale_ai::AiAdapter>)));

        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        let (bot_msg_tx, _bot_msg_rx) = mpsc::channel(256);
        let (chat_tx, _chat_rx) = tokio::sync::broadcast::channel(256);
        let trigger_registry =
            springtale_runtime::TriggerRegistry::new(trigger_tx.clone(), store.clone());

        let heartbeat_monitor = Arc::new(Mutex::new(springtale_scheduler::HeartbeatMonitor::new(
            0,
            trigger_tx.clone(),
        )));

        let canvas = Arc::new(RwLock::new(springtale_core::canvas::CanvasState::default()));
        let (canvas_tx, _rx) = tokio::sync::broadcast::channel(64);
        let (notification_tx, _notif_rx) = tokio::sync::broadcast::channel(256);
        let (cooperation_tx, _coop_rx) = tokio::sync::broadcast::channel(512);
        let formation_gossip: Arc<dyn springtale_cooperation::gossip::FormationGossipBus> =
            springtale_cooperation::gossip::InMemoryFormationGossipBus::new();
        let knowledge_store: Arc<dyn springtale_cooperation::memory::GlobalKnowledgeStore> =
            springtale_cooperation::memory::InMemoryKnowledgeStore::new();

        let wasm_engine = Arc::new(
            springtale_connector::wasm::WasmEngine::new(
                springtale_connector::wasm::SandboxLimits::default(),
            )
            .expect("WASM engine creation"),
        );
        let wasm_tier_cache = Arc::new(
            springtale_connector::wasm::WasmTierCache::new(wasm_engine.clone())
                .expect("WASM tier cache init"),
        );

        let (formation_cmd_tx, formation_cmd_rx) = mpsc::channel::<FormationCommand>(32);

        let gossip_store: Arc<dyn springtale_cooperation::awareness::GossipStore> =
            Arc::new(springtale_cooperation::awareness::InMemoryGossipStore::new());
        let capability_bridge = springtale_runtime::CapabilityBridge::new(registry.clone());
        let (bot_chat_tx, bot_chat_rx) = mpsc::channel(64);
        let role_registry = Arc::new(springtale_cooperation::role::RoleRegistry::with_builtins());
        let runtime = springtale_runtime::RuntimeState {
            store,
            registry,
            engine,
            ai_adapter,
            bot_settings: Arc::new(arc_swap::ArcSwap::from_pointee(Default::default())),
            sentinel,
            wasm_engine,
            wasm_tier_cache,
            capability_bridge,
            role_registry,
            canvas,
            canvas_tx,
            notification_tx,
            event_tx: event_tx.clone(),
            trigger_registry: Arc::new(std::sync::OnceLock::new()),
            cooperation_tx,
            utterances: Default::default(),
            utterance_defs: Default::default(),
            cadence_tick: Default::default(),
            formation_cmd_tx,
            live_formations: None,
            gossip_store,
            formation_gossip,
            knowledge_store,
            // Single-process test fixture — no SWIM node.
            swim_node: None,
            chat_tx: bot_chat_tx,
            chat_rx: Arc::new(tokio::sync::Mutex::new(Some(bot_chat_rx))),
            chat_tasks: Default::default(),
            // In-memory store — no runtime lock.
            _lock: None,
        };

        // The producer is a stub — these fixtures drive the API surface,
        // not the job consumer.
        let (job_tx, _job_rx) = mpsc::channel::<springtale_scheduler::Job>(16);
        let producer = Arc::new(springtale_scheduler::JobProducer::new(job_tx));
        let scheduler = springtale_runtime::EmbeddedScheduler {
            cron,
            fs_watcher,
            trigger_tx: trigger_tx.clone(),
            producer,
        };

        let state = AppState {
            runtime,
            api_token_hash,
            ready: ready_flag,
            trigger_tx,
            scheduler,
            rate_limit_per_sec: 1000,
            event_tx,
            heartbeat_monitor,
            bot_msg_tx,
            trigger_registry,
            chat_tx,
            stream_tickets: Arc::new(Mutex::new(HashMap::new())),
        };

        Self {
            router: build_router(state.clone()),
            token_hex,
            formation_cmd_rx,
            state,
        }
    }
}
