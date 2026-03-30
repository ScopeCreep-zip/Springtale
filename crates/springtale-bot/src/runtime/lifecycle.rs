use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::{RwLock, mpsc};

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_store::StorageBackend;

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
    pub(crate) connector_rx: mpsc::Receiver<IncomingMessage>,
    pub(crate) rule_rx: mpsc::Receiver<TriggerEvent>,
    pub(crate) response_tx: mpsc::Sender<OutgoingResponse>,
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

        // Build conversation context
        let context = ConversationContext::new(store.clone(), self.config.context_window);

        Ok(Bot {
            router,
            handlers,
            store,
            registry,
            engine,
            config: self.config,
            context,
            connector_rx,
            rule_rx,
            response_tx,
        })
    }
}

impl Default for BotBuilder {
    fn default() -> Self {
        Self::new()
    }
}
