use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::{RwLock, broadcast, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_store::StorageBackend;

use crate::cooperation::cadence::{CadenceBus, Tick};
use crate::cooperation::formation::Formation;
use crate::error::BotError;
use crate::handler::HandlerRegistry;
use crate::memory::ConversationContext;
use crate::router::Router;
use crate::state::persona::BotPersona;

/// Incoming message from any chat connector.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub user_id: String,
    pub channel_id: String,
    pub text: String,
    /// Which connector this came from (for reply routing).
    pub source_connector: String,
    /// Raw connector payload.
    pub raw: serde_json::Value,
}

/// Outgoing response to be sent via a connector.
#[derive(Debug, Clone)]
pub struct OutgoingResponse {
    pub channel_id: String,
    pub text: String,
    pub connector: String,
}

/// Configuration for the bot runtime.
#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    /// Conversation context window size. Default: 50.
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    /// Bot persona configuration.
    #[serde(default)]
    pub persona: BotPersona,
    /// Vault auto-lock timeout in seconds. Default: 300 (5 min).
    #[serde(default = "default_vault_timeout")]
    pub vault_timeout_secs: u64,
    /// Controls which connector actions the AI can call as tools.
    /// Default: empty allow list = AI has zero tools (OWASP LLM06).
    #[serde(default)]
    pub tool_policy: springtale_ai::ToolPolicy,
}

fn default_context_window() -> usize {
    50
}

fn default_vault_timeout() -> u64 {
    300
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            context_window: default_context_window(),
            persona: BotPersona::default(),
            vault_timeout_secs: default_vault_timeout(),
            tool_policy: springtale_ai::ToolPolicy::default(),
        }
    }
}

/// The bot runtime. Owns all bot subsystems.
pub struct Bot {
    pub(crate) router: Router,
    pub(crate) handlers: HandlerRegistry,
    pub(crate) store: Arc<dyn StorageBackend>,
    pub(crate) registry: Arc<RwLock<ConnectorRegistry>>,
    pub(crate) engine: Arc<RwLock<RuleEngine>>,
    pub(crate) config: BotConfig,
    pub(crate) context: ConversationContext,
    pub(crate) ai_adapter: Arc<dyn springtale_ai::AiAdapter>,
    pub(crate) sentinel: Arc<springtale_sentinel::Sentinel>,
    pub(crate) connector_rx: mpsc::Receiver<IncomingMessage>,
    pub(crate) rule_rx: mpsc::Receiver<TriggerEvent>,
    pub(crate) response_tx: mpsc::Sender<OutgoingResponse>,
    /// Active formations (cooperation module).
    pub(crate) formations: Arc<RwLock<Vec<Formation>>>,
    /// Cadence bus — used by orchestrator::composer when creating formations.
    #[allow(dead_code)]
    pub(crate) cadence: CadenceBus,
    /// Receiver for cadence ticks (event loop select! branch).
    pub(crate) cadence_rx: broadcast::Receiver<Tick>,
}

impl Bot {
    /// Start the bot event loop. Runs until all channels are closed.
    pub async fn start(mut self) {
        crate::runtime::event_loop::run_event_loop(&mut self).await;
    }
}

/// Builder for constructing a `Bot` instance.
pub struct BotBuilder {
    store: Option<Arc<dyn StorageBackend>>,
    registry: Option<Arc<RwLock<ConnectorRegistry>>>,
    engine: Option<Arc<RwLock<RuleEngine>>>,
    config: BotConfig,
    ai_adapter: Option<Arc<dyn springtale_ai::AiAdapter>>,
    sentinel: Option<Arc<springtale_sentinel::Sentinel>>,
    connector_rx: Option<mpsc::Receiver<IncomingMessage>>,
    rule_rx: Option<mpsc::Receiver<TriggerEvent>>,
    response_tx: Option<mpsc::Sender<OutgoingResponse>>,
}

impl BotBuilder {
    pub fn new() -> Self {
        Self {
            store: None,
            registry: None,
            engine: None,
            config: BotConfig::default(),
            ai_adapter: None,
            sentinel: None,
            connector_rx: None,
            rule_rx: None,
            response_tx: None,
        }
    }

    pub fn store(mut self, store: Arc<dyn StorageBackend>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn registry(mut self, registry: Arc<RwLock<ConnectorRegistry>>) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn engine(mut self, engine: Arc<RwLock<RuleEngine>>) -> Self {
        self.engine = Some(engine);
        self
    }

    pub fn config(mut self, config: BotConfig) -> Self {
        self.config = config;
        self
    }

    pub fn connector_rx(mut self, rx: mpsc::Receiver<IncomingMessage>) -> Self {
        self.connector_rx = Some(rx);
        self
    }

    pub fn rule_rx(mut self, rx: mpsc::Receiver<TriggerEvent>) -> Self {
        self.rule_rx = Some(rx);
        self
    }

    pub fn response_tx(mut self, tx: mpsc::Sender<OutgoingResponse>) -> Self {
        self.response_tx = Some(tx);
        self
    }

    pub fn ai_adapter(mut self, adapter: Arc<dyn springtale_ai::AiAdapter>) -> Self {
        self.ai_adapter = Some(adapter);
        self
    }

    pub fn sentinel(mut self, sentinel: Arc<springtale_sentinel::Sentinel>) -> Self {
        self.sentinel = Some(sentinel);
        self
    }

    /// Build the bot, initializing router, handlers, and context.
    pub async fn build(self) -> Result<Bot, BotError> {
        let store = self
            .store
            .ok_or_else(|| BotError::NotInitialized("store required".into()))?;
        let registry = self
            .registry
            .ok_or_else(|| BotError::NotInitialized("registry required".into()))?;
        let engine = self
            .engine
            .ok_or_else(|| BotError::NotInitialized("engine required".into()))?;
        let connector_rx = self
            .connector_rx
            .ok_or_else(|| BotError::NotInitialized("connector_rx required".into()))?;
        let rule_rx = self
            .rule_rx
            .ok_or_else(|| BotError::NotInitialized("rule_rx required".into()))?;
        let response_tx = self
            .response_tx
            .ok_or_else(|| BotError::NotInitialized("response_tx required".into()))?;

        // Load aliases from store
        let alias_pairs = store.list_aliases().await?;
        let aliases = alias_pairs.into_iter().collect();

        // Build router
        let mut router = Router::new(aliases, vec![]);

        // Build handler registry with builtins
        let mut handlers = HandlerRegistry::new();
        crate::handler::builtin::register_builtins(&mut handlers)?;

        // Register builtin command names in prefix router
        for cmd in crate::handler::builtin::BUILTIN_COMMANDS {
            router.register_command(cmd);
        }

        // Auto-register connector actions as commands
        {
            let reg = registry.read().await;
            crate::handler::connector::auto_register_connector_commands(
                &mut handlers,
                router.prefix_mut(),
                &reg,
            )?;
        }

        // AI adapter — defaults to NoopAdapter if not provided
        let ai_adapter: Arc<dyn springtale_ai::AiAdapter> = self
            .ai_adapter
            .unwrap_or_else(|| Arc::new(springtale_ai::NoopAdapter));

        // Derive or load memory encryption key from config store.
        // The key persists across restarts so encrypted messages remain readable.
        let encryption_key = match store.get_config("memory:encryption_key").await {
            Ok(Some(key_hex)) => {
                let bytes = hex::decode(key_hex.trim_matches('"'))
                    .map_err(|e| BotError::Memory(format!("invalid encryption key hex: {e}")))?;
                let key: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| BotError::Memory("encryption key must be 32 bytes".into()))?;
                key
            }
            _ => {
                // First run: generate a random key and persist it
                use rand::RngCore;
                let mut key = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                let key_hex = hex::encode(key);
                if let Err(e) = store
                    .set_config("memory:encryption_key", &format!("\"{key_hex}\""))
                    .await
                {
                    tracing::warn!(error = %e, "failed to persist memory encryption key");
                }
                key
            }
        };

        let context = ConversationContext::new(
            store.clone(),
            self.config.context_window,
            ai_adapter.clone(),
            encryption_key,
        );

        // Create cadence bus for cooperation (§5)
        let (cadence_tx, _) = broadcast::channel::<Tick>(64);
        let cadence = CadenceBus::new(Duration::from_secs(1), cadence_tx);
        let cadence_rx = cadence.subscribe();

        // Spawn cadence tick task (external clock, Necrodancer pattern)
        let cadence_clone = cadence.clone();
        tokio::spawn(async move {
            cadence_clone.run().await;
        });

        Ok(Bot {
            router,
            handlers,
            store,
            registry,
            engine,
            config: self.config,
            context,
            ai_adapter,
            sentinel: self
                .sentinel
                .ok_or_else(|| BotError::NotInitialized("sentinel required".into()))?,
            connector_rx,
            rule_rx,
            response_tx,
            formations: Arc::new(RwLock::new(Vec::new())),
            cadence,
            cadence_rx,
        })
    }
}

impl Default for BotBuilder {
    fn default() -> Self {
        Self::new()
    }
}
